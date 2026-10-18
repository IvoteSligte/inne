pub mod ast;
pub mod hir;
pub mod interpreter;
pub mod mir;
pub mod typed_hir;

pub use ast::{Ast, ast_from_parse_tree};
pub use error::*;
pub use hir::{Hir, hir_from_ast};
pub use interpreter::Interpreter;
pub use mir::{Mir, mir_from_typed_hir};
pub use parser::{
    Input as _, LexError, ParseError, ParseTree, Parser as _, Rich, Span, Spanned, Token,
};
pub use typed_hir::{TypedHir, typed_hir_from_hir};

pub fn lex(source: &str) -> (Option<Vec<(Token<'_>, Span)>>, Vec<Error>) {
    let (tokens, errors) = parser::create_lexer().parse(source).into_output_errors();
    let errors = errors.into_iter().map(Error::from_parse_error).collect();
    (tokens, errors)
}

pub fn parse<'src>(
    tokens: &[(Token<'src>, Span)],
    source: &'src str,
    filename: &'src str,
) -> (Option<ParseTree<'src>>, Vec<Error>) {
    let (tokens, errors) = parser::create_parser()
        .map(|stmts| ParseTree { filename, stmts })
        .parse(tokens.map((source.len()..source.len()).into(), |(t, s)| (t, s)))
        .into_output_errors();
    let errors = errors.into_iter().map(Error::from_parse_error).collect();
    (tokens, errors)
}
