use std::collections::BTreeMap;

use inne::ast::*;
use inne::*;
use tower_lsp_server::ls_types::SemanticToken;

use crate::{File, position_at_end};

use super::tt;

pub fn extract_semantic_tokens(file: &File) -> Option<Vec<SemanticToken>> {
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
            Token::Int(_) | Token::Bool(_) => tt::NUMBER,
            Token::String(_) | Token::Char(_) => tt::STRING,
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
    Some(data)
}

struct Override<'a>(&'a mut BTreeMap<usize, (Span, u32)>);

impl Override<'_> {
    fn set(&mut self, token_type: u32, span: Span) {
        debug_assert!(self.0.contains_key(&span.start));
        self.0.insert(span.start, (span, token_type));
    }

    fn stmt(&mut self, stmt: &parser::Stmt) {
        match stmt {
            Stmt::Func(Func {
                name,
                generics,
                params,
                return_type,
                body,
            }) => {
                self.set(tt::FUNCTION, name.1);
                for generic in generics {
                    self.set(tt::TYPE, generic.1);
                }
                for param in params {
                    self.set(tt::PARAMETER, param.name.1);
                    self.set(tt::TYPE, param.r#type.1);
                }
                if let Some((_, type_span)) = return_type {
                    self.set(tt::TYPE, *type_span);
                }
                for stmt in body {
                    self.stmt(stmt);
                }
            }
            Stmt::Struct(Struct { name, fields }) => {
                self.set(tt::STRUCT, name.1);
                for Field { name, r#type } in fields {
                    self.set(tt::PROPERTY, name.1);
                    self.r#type(r#type.1);
                }
            }
            Stmt::Enum(Enum { name, variants }) => {
                self.set(tt::ENUM, name.1);
                for Variant { name, payload } in variants {
                    self.set(tt::ENUM_MEMBER, name.1);
                    if let Some(payload) = payload.as_ref() {
                        self.r#type(payload.1);
                    }
                }
            }
            Stmt::VarDecl(VarDecl { name, value }) => {
                self.set(tt::VARIABLE, name.1);
                self.expr(value);
            }
            Stmt::Expr(expr) => self.expr(expr),
            Stmt::Assign(Assign {
                target,
                op: _,
                value,
            }) => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::If(If {
                cond,
                then,
                else_ifs,
                r#else,
            }) => {
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
            Stmt::For(For { var: _, iter, body }) => {
                self.expr(iter);
                for stmt in body {
                    self.stmt(stmt);
                }
            }
            Stmt::While(While { cond, body }) => {
                self.expr(cond);
                for stmt in body {
                    self.stmt(stmt);
                }
            }
            Stmt::Loop(Loop { body }) => {
                for stmt in body {
                    self.stmt(stmt);
                }
            }
            Stmt::Return((Some(expr), _)) => {
                self.expr(expr);
            }
            Stmt::Return((None, _)) => {}
            Stmt::Break(_) => {}
            Stmt::Continue(_) => {}
        }
    }

    fn expr(&mut self, (expr, span): &Spanned<Expr>) {
        match expr {
            Expr::Binary(Binary { left, op: _, right }) => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Unary(Unary { op: _, expr }) => self.expr(expr),
            Expr::Call(Call { name, args }) => {
                self.set(tt::FUNCTION, name.1);
                for arg in args {
                    self.expr(arg);
                }
            }
            Expr::Var(_) => {} // VARIABLE is the default
            Expr::Int(_) => {}
            Expr::Bool(_) => {}
            Expr::String(_) => {}
            Expr::Char(_) => {}
            Expr::Range(Range { start, end }) => {
                self.expr(start);
                self.expr(end);
            }
            Expr::EmptyList(_) => self.r#type((span.start + 1..span.end + 1).into()),
            Expr::List(List { elements }) => elements.iter().for_each(|item| self.expr(&item)),
            Expr::EmptyMap(_) => {
                self.r#type((span.start + 1..span.end + 1).into());
            }
            Expr::Map(Map { pairs }) => pairs.iter().for_each(|Pair { key, value }| {
                self.expr(key);
                self.expr(value);
            }),
            Expr::Index(Index { expr, index }) => {
                self.expr(&expr);
                self.expr(&index);
            }
            Expr::StructLiteral(StructLiteral { name, fields }) => {
                self.set(tt::STRUCT, name.1);
                for FieldLiteral { name, value } in fields {
                    self.set(tt::PROPERTY, name.1);
                    self.expr(value);
                }
            }
            Expr::VariantLiteral(VariantLiteral {
                enum_name,
                name,
                payload,
            }) => {
                self.set(tt::ENUM, enum_name.1);
                self.set(tt::ENUM_MEMBER, name.1);
                if let Some(payload) = payload.as_ref() {
                    self.expr(payload);
                }
            }
            Expr::Member(Member { expr, member }) => {
                self.expr(expr);
                self.set(tt::PROPERTY, member.1);
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
