pub mod hir;
pub mod interpreter;
pub mod mir;
pub mod typed_hir;
mod util;
pub mod value;

pub use error::Errors;
pub use error::*;
pub use hir::{Hir, hir_from_ast};
pub use interpreter::Interpreter;
pub use mir::{Mir, mir_from_typed_hir};
pub use parser::{
    self, Ast, Input as _, LexError, ParseError, Parser as _, Rich, Span, Spanned, Token, ast,
};
pub use typed_hir::{TypedHir, typed_hir_from_hir};
pub use value::Value;

pub fn lex(source: &str) -> (Option<Vec<(Token<'_>, Span)>>, Vec<Error>) {
    let (tokens, errors) = parser::lex(source);
    let errors = errors
        .into_iter()
        .map(Error::from_lex_error)
        .collect::<Vec<_>>();
    (errors.is_empty().then_some(tokens), errors)
}

pub fn parse<'src>(
    tokens: &[(Token<'src>, Span)],
    source: &'src str,
    filename: &'src str,
) -> (Option<Ast<'src>>, Vec<Error>) {
    let tokens = tokens
        .iter()
        .filter(|(token, _)| !matches!(token, Token::Comment(_)))
        .collect::<Vec<_>>();
    let (tokens, errors) = parser::create_parser()
        .map(|stmts| Ast { filename, stmts })
        .parse(tokens.map((source.len()..source.len()).into(), |(t, s)| (t, s)))
        .into_output_errors();
    let errors = errors.into_iter().map(Error::from_parse_error).collect();
    (tokens, errors)
}
