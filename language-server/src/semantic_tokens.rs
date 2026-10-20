use std::collections::BTreeMap;

use inne::*;
use tower_lsp_server::ls_types::*;

use crate::{File, position_at_end};

use super::tt;

pub fn extract_semantic_tokens(file: &File) -> Option<SemanticTokensResult> {
    if file.source.is_empty() {
        eprintln!("Semantic tokens: File '{}' is empty.", file.name);
        return None;
    }
    let (Some(tokens), _) = inne::lex(&file.source) else {
        eprintln!("Semantic tokens: Failed to lex file '{}'.", file.name);
        return None;
    };
    // First pass reports tokens according to tokens.
    // Using a BTreeMap instead of HashMap to ensure ordering is retained.
    let mut semantic_tokens = BTreeMap::from_iter(tokens.iter().filter_map(|(token, span)| {
        let token_type = match token {
            Token::Ident(_) => tt::VARIABLE, // default for identifiers
            Token::Int(_) => tt::NUMBER,
            Token::Bool(_) => tt::NUMBER,
            Token::String(_) => tt::STRING,
            Token::Op("and") | Token::Op("or") | Token::Op("not") => tt::KEYWORD,
            Token::Op(_) => tt::OPERATOR,
            Token::Comment(_) => tt::COMMENT,
            _ if token.is_keyword() => tt::KEYWORD,
            _ => return None,
        };
        Some((span.start, (*span, token_type)))
    }));
    // Second pass overrides identifiers with their specialised types (if applicable)
    if let (Some(parse_tree), _) = inne::parse(&tokens, &file.source, &file.name) {
        let mut r#override = Override(&mut semantic_tokens);
        for stmt in &parse_tree.stmts {
            r#override.stmt(stmt);
        }
    } else {
        eprintln!("Semantic tokens: Failed to parse file '{}'.", file.name);
    }
    let data = to_semantic_tokens(&file.source, semantic_tokens.into_values());
    eprintln!("Semantic tokens (full) provided {} tokens.", data.len());
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

struct Override<'a>(&'a mut BTreeMap<usize, (Span, u32)>);

impl Override<'_> {
    fn set(&mut self, token_type: u32, span: Span) {
        debug_assert!(self.0.contains_key(&span.start));
        self.0.insert(span.start, (span, token_type));
    }

    fn stmt(&mut self, stmt: &parser::Stmt) {
        match stmt {
            parser::Stmt::Func((_, name_span), params, return_type, body) => {
                self.set(tt::FUNCTION, *name_span);
                for param in params {
                    let (_, name_span) = param.0;
                    self.set(tt::PARAMETER, name_span);
                    let (_, type_span) = param.1;
                    self.set(tt::TYPE, type_span);
                }
                if let Some((_, type_span)) = return_type {
                    self.set(tt::TYPE, *type_span);
                }
                for stmt in body {
                    self.stmt(stmt);
                }
            }
            parser::Stmt::VarDecl((_, name_span), expr) => {
                self.set(tt::VARIABLE, *name_span);
                self.expr(expr);
            }
            parser::Stmt::Expr(expr) => self.expr(expr),
            parser::Stmt::Assign(target, _, expr) => {
                self.expr(target);
                self.expr(expr);
            }
            parser::Stmt::If(cond, then, else_ifs, r#else) => {
                self.expr(cond);
                for stmt in then {
                    self.stmt(stmt);
                }
                for (cond, then) in else_ifs {
                    self.expr(cond);
                    for stmt in then {
                        self.stmt(stmt);
                    }
                }
                if let Some(r#else) = r#else {
                    for stmt in r#else {
                        self.stmt(stmt);
                    }
                }
            }
            parser::Stmt::For(_, iter, stmts) => {
                self.expr(iter);
                for stmt in stmts {
                    self.stmt(stmt);
                }
            }
            parser::Stmt::While(cond, stmts) => {
                self.expr(cond);
                for stmt in stmts {
                    self.stmt(stmt);
                }
            }
            parser::Stmt::Loop(stmts) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
            }
            parser::Stmt::Return((Some(expr), _)) => {
                self.expr(expr);
            }
            parser::Stmt::Return((None, _)) => {}
            parser::Stmt::Break(_) => {}
            parser::Stmt::Continue(_) => {}
        }
    }

    fn expr(&mut self, (expr, span): &Spanned<parser::Expr>) {
        match expr {
            parser::Expr::Binary(left, _, right) => {
                self.expr(&left);
                self.expr(&right);
            }
            parser::Expr::Unary(_, value) => self.expr(&value),
            parser::Expr::Call((_, name_span), (args, _)) => {
                self.set(tt::FUNCTION, *name_span);
                for arg in args {
                    self.expr(arg);
                }
            }
            parser::Expr::Var(_) => {} // VARIABLE is the default
            parser::Expr::Int(_) => {}
            parser::Expr::Bool(_) => {}
            parser::Expr::String(_) => {}
            parser::Expr::Range(start, end) => {
                self.expr(&start);
                self.expr(&end);
            }
            parser::Expr::EmptyList(_) => self.r#type((span.start + 1..span.end + 1).into()),
            parser::Expr::List(elements) => elements.iter().for_each(|item| self.expr(&item)),
            parser::Expr::EmptyMap(_, _) => {
                self.r#type((span.start + 1..span.end + 1).into());
            }
            parser::Expr::Map(pairs) => pairs.iter().for_each(|(key, value)| {
                self.expr(&key);
                self.expr(&value);
            }),
            parser::Expr::Index(expr, index) => {
                self.expr(&expr);
                self.expr(&index);
            }
        }
    }

    fn r#type(&mut self, span: Span) {
        // overrides < and > as tt::DELIMITER and idents as tt::TYPE
        self.0
            .range_mut(span.into_range())
            .for_each(|(_, (span, token_type))| {
                if *token_type == tt::VARIABLE {
                    // identifier
                    *token_type = tt::TYPE;
                    eprintln!("{span} => tt::TYPE");
                } else if *token_type == tt::OPERATOR {
                    // < or >
                    *token_type = tt::DELIMITER;
                    eprintln!("{span} => tt::DELIMITER");
                }
            });
    }
}

fn to_semantic_tokens(
    source: &str,
    tokens: impl IntoIterator<Item = (Span, u32)>,
) -> Vec<SemanticToken> {
    let mut start_byte = 0usize;
    tokens
        .into_iter()
        .map(|(span, token_type)| {
            let delta_position = position_at_end(&source[start_byte..span.start], &mut 0, &mut 0);
            start_byte = span.start;
            SemanticToken {
                delta_line: delta_position.line,
                delta_start: delta_position.character,
                length: (span.end - span.start) as _,
                token_type,
                token_modifiers_bitset: 0,
            }
        })
        .collect()
}
