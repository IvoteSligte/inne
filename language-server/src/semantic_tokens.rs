use std::{collections::BTreeMap, time::Instant};

use inne::ast::*;
use inne::*;
use tower_lsp_server::ls_types::{SemanticToken, SemanticTokensEdit};

use crate::{File, position_at_end};

use super::tt;

pub fn extract_semantic_tokens(file: &File) -> Option<Vec<SemanticToken>> {
    let start_instant = Instant::now();
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
    let data = to_semantic_tokens(&file.source, semantic_tokens.into_values()).collect::<Vec<_>>();
    let end_instant = Instant::now();
    let duration = end_instant - start_instant;
    eprintln!(
        "Semantic tokens ({duration:?}, version {}) provided {} tokens.",
        file.version,
        data.len()
    );
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

fn to_semantic_tokens<'src>(
    source: &'src str,
    tokens: impl IntoIterator<Item = (Span, u32)>,
) -> impl Iterator<Item = SemanticToken> {
    let mut start_byte = 0usize;
    tokens.into_iter().map(move |(span, token_type)| {
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
}

fn search(slice: &[SemanticToken], pattern: &[SemanticToken]) -> Option<usize> {
    for (i, chunk) in slice.chunks_exact(pattern.len()).enumerate() {
        if chunk == pattern {
            return Some(i);
        }
    }
    None
}

const PATTERN_LEN: usize = 3;
const MAX_SEARCH: usize = 10;

// TODO: aggregate consecutive edits
pub fn compute_delta<'a>(old: &[SemanticToken], new: &[SemanticToken]) -> Vec<SemanticTokensEdit> {
    let mut edits = Vec::new();
    let mut old_i = 0;
    let mut new_i = 0;

    let mut add_edit = |start: usize, delete_count: usize, data: Option<_>| {
        edits.push(SemanticTokensEdit {
            // These values need to be multiplied by 5 because they are written in terms of
            // integers in the flattened JSON-encoded array (5 per SemanticToken).
            start: (start * 5) as u32,
            delete_count: (delete_count * 5) as u32,
            data,
        });
    };
    loop {
        let Some(&new_token) = new.get(new_i) else {
            if old_i < old.len() {
                add_edit(old_i, old.len() - old_i, None);
            }
            break;
        };
        let Some(&old_token) = old.get(old_i) else {
            if new_i < new.len() {
                add_edit(old_i, 0, Some(new[new_i..].to_vec()));
            }
            break;
        };
        if old_token == new_token {
            old_i += 1;
            new_i += 1;
            continue;
        }
        if let Some(offset) = search(
            &old[old_i + 1..(old_i + 1 + MAX_SEARCH + PATTERN_LEN - 1).min(old.len())],
            &new[new_i..(new_i + PATTERN_LEN).min(new.len())],
        ) {
            add_edit(old_i, 1 + offset, Some(vec![new_token]));
            old_i += 1 + offset;
            new_i += 1;
            continue;
        }
        add_edit(old_i, 1, Some(vec![new_token]));
        old_i += 1;
        new_i += 1;
    }
    edits
}
