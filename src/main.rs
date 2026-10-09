use std::sync::Arc;

pub mod ast;
mod lexer;
mod parser;

use ariadne::{Color, Label, Report, ReportKind, sources};
use chumsky::{Parser as _, error::Rich, input::Input};

use ast::parse_tree_to_ast;

pub use lexer::{Span, Spanned, Token, lexer};
pub use parser::{ParseTree, parser};

pub type Source = Arc<String>;

fn print_errors<T, E>(
    result: Result<T, Vec<Rich<'_, E>>>,
    filename: &'static str,
    source: &str,
    stage: &'static str,
) -> Option<T>
where
    E: std::fmt::Display,
{
    let errors = match result {
        Ok(ok) => return Some(ok),
        Err(errors) => errors,
    };
    for err in errors {
        Report::build(ReportKind::Error, (filename, err.span().into_range()))
            .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
            .with_message(format!("during {stage}, {}", err))
            .with_label(
                Label::new((filename, err.span().into_range()))
                    .with_message(err.reason())
                    .with_color(Color::Red),
            )
            .with_labels(err.contexts().map(|(label, span)| {
                Label::new((filename, span.into_range()))
                    .with_message(format!("while parsing this {label}"))
                    .with_color(Color::Yellow)
            }))
            .finish()
            .print(sources([(filename, source)]))
            .unwrap()
    }
    None
}

fn main() {
    let source = "var x = 0;
x += 1;

func hello_world() {
    print(\"Hello, World!\");
}
";
    let filename = "filename.inne";
    let Some(tokens) = print_errors(
        lexer().parse(source).into_result(),
        filename,
        source,
        "lexing",
    ) else {
        return;
    };
    let Some(parse_tree) = print_errors(
        parser()
            .parse(
                tokens
                    .as_slice()
                    .map((source.len()..source.len()).into(), |(t, s)| (t, s)),
            )
            .into_result(),
        filename,
        source,
        "parsing",
    ) else {
        return;
    };
    let ast = parse_tree_to_ast(&parse_tree);
}
