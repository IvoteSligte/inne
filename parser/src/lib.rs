pub mod diagnosis;
pub mod lexer;
pub mod parser;

pub use lexer::*;
pub use parser::*;

pub use chumsky::{Parser, error::Rich, input::Input};

pub fn lex(source: &str) -> (Option<Vec<(Token<'_>, Span)>>, Vec<LexError<'_>>) {
    create_lexer().parse(source).into_output_errors()
}

pub fn parse<'tokens: 'src, 'src>(
    tokens: &'tokens [(Token<'src>, Span)],
    source: &'src str,
    filename: &'src str,
) -> (Option<ParseTree<'src>>, Vec<ParseError<'tokens, 'src>>) {
    create_parser()
        .map(|stmts| ParseTree { filename, stmts })
        .parse(tokens.map((source.len()..source.len()).into(), |(t, s)| (t, s)))
        .into_output_errors()
}
