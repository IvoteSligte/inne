use std::collections::BTreeMap;

use inne::*;
use tower_lsp_server::ls_types::*;

use crate::File;

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
            _ if token.is_keyword() => tt::KEYWORD,
            _ => return None,
        };
        Some((*span, token_type))
    }));
    // Second pass overrides identifiers with their specialised types (if applicable)
    if let (Some(parse_tree), _) = inne::parse(&tokens, &file.source, &file.name) {
        for stmt in &parse_tree.stmts {
            stmt_extract_idents(stmt, &mut |token_type, span| {
                semantic_tokens.insert(span, token_type);
            });
        }
    } else {
        eprintln!("Semantic tokens: Failed to parse file '{}'.", file.name);
    }
    let data = to_semantic_tokens(&file.source, semantic_tokens);
    eprintln!("Semantic tokens (full) provided {} tokens.", data.len());
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

fn stmt_extract_idents(stmt: &parser::Stmt, set: &mut impl FnMut(u32, Span)) {
    match stmt {
        parser::Stmt::Func((_, name_span), params, return_type, body) => {
            set(tt::FUNCTION, *name_span);
            for param in params {
                let (_, name_span) = param.0;
                set(tt::PARAMETER, name_span);
                let (_, type_span) = param.1;
                set(tt::TYPE, type_span);
            }
            if let Some((_, type_span)) = return_type {
                set(tt::TYPE, *type_span);
            }
            for stmt in body {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::VarDecl((_, name_span), (expr, _)) => {
            set(tt::VARIABLE, *name_span);
            expr_extract_idents(expr, set);
        }
        parser::Stmt::Expr((expr, _)) => expr_extract_idents(expr, set),
        parser::Stmt::Assign((target, _), _, (expr, _)) => {
            expr_extract_idents(target, set);
            expr_extract_idents(expr, set);
        }
        parser::Stmt::If((cond, _), then, else_ifs, r#else) => {
            expr_extract_idents(cond, set);
            for stmt in then {
                stmt_extract_idents(stmt, set);
            }
            for ((cond, _), then) in else_ifs {
                expr_extract_idents(cond, set);
                for stmt in then {
                    stmt_extract_idents(stmt, set);
                }
            }
            if let Some(r#else) = r#else {
                for stmt in r#else {
                    stmt_extract_idents(stmt, set);
                }
            }
        }
        parser::Stmt::For(_, (iter, _), stmts) => {
            expr_extract_idents(iter, set);
            for stmt in stmts {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::While((cond, _), stmts) => {
            expr_extract_idents(cond, set);
            for stmt in stmts {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::Loop(stmts) => {
            for stmt in stmts {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::Return((Some((expr, _)), _)) => {
            expr_extract_idents(expr, set);
        }
        parser::Stmt::Return((None, _)) => {}
        parser::Stmt::Break(_) => {}
        parser::Stmt::Continue(_) => {}
    }
}

fn expr_extract_idents(expr: &parser::Expr, set: &mut impl FnMut(u32, Span)) {
    match expr {
        parser::Expr::Binary(left, _, right) => {
            expr_extract_idents(&left.0, set);
            expr_extract_idents(&right.0, set);
        }
        parser::Expr::Unary(_, value) => expr_extract_idents(&value.0, set),
        parser::Expr::Call((_, name_span), (args, _)) => {
            set(tt::FUNCTION, *name_span);
            for (arg, _) in args {
                expr_extract_idents(arg, set);
            }
        }
        parser::Expr::Var(_) => {} // VARIABLE is the default
        parser::Expr::Int(_) => {}
        parser::Expr::Bool(_) => {}
        parser::Expr::String(_) => {}
        parser::Expr::Range(start, end) => {
            expr_extract_idents(&start.0, set);
            expr_extract_idents(&end.0, set);
        }
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
            let mut delta_line = 0;
            let mut delta_start = 0; // character/column
            for c in source[start_byte..span.start].chars() {
                if c == '\n' {
                    delta_line += 1;
                    delta_start = 0;
                } else {
                    delta_start += 1;
                }
            }
            start_byte = span.start;
            let length = (span.end - span.start) as _;
            SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: 0,
            }
        })
        .collect()
}
