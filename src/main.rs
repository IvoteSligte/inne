use std::{ffi::OsStr, io, path::PathBuf};

pub mod analyzer;
pub mod ast;
mod lexer;
mod parser;

use ariadne::{Color, Label, Report, ReportKind, sources};
use chumsky::{Parser as _, error::Rich, input::Input};

use ast::parse_tree_to_ast;

pub use analyzer::analyze;
use clap::Parser;
pub use lexer::{Span, Spanned, Token, lexer};
pub use parser::{ParseTree, parser};

#[derive(clap::Parser)]
#[command(name = "Inne")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Compiler for the Inne programming language", long_about = None)]
struct Args {
    /// The name of the Inne file to run.
    #[arg(help = "The name of the Inne file to run.")]
    file: PathBuf,
}

// TODO: display general diagnostics through ariadne as well

fn main() {
    let args = Args::parse();
    // Human-readable filename.
    let filename = args.file.as_os_str().to_string_lossy();
    let source = match std::fs::read_to_string(&args.file) {
        Ok(source) => source,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            eprintln!("Error: The file '{}' does not exist.", filename);
            return;
        }
        Err(err) if err.kind() == io::ErrorKind::IsADirectory => {
            eprintln!("Error: '{}' is a directory, not a '.inne' file.", filename);
            return;
        }
        Err(err) => {
            eprintln!("Error: Failed to read file '{}': {}", filename, err);
            return;
        }
    };
    if args.file.extension() != Some(OsStr::new("inne")) {
        eprintln!(
            "Warning: The file '{}' does not seem to be a '.inne' file. Trying to run it anyways.",
            filename
        );
    }
    let filename = "filename.inne";
    let Some(tokens) = print_parse_errors(
        lexer().parse(&source).into_result(),
        filename,
        &source,
        "lexing",
    ) else {
        return;
    };
    let Some(parse_tree) = print_parse_errors(
        parser()
            .parse(
                tokens
                    .as_slice()
                    .map((source.len()..source.len()).into(), |(t, s)| (t, s)),
            )
            .into_result(),
        filename,
        &source,
        "parsing",
    ) else {
        return;
    };
    let ast = parse_tree_to_ast(&parse_tree);
    let errors = analyze(&ast, filename);

    print_analyze_errors(&source, filename, errors);
}

fn print_parse_errors<T, E>(
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
            .eprint(sources([(filename, source)]))
            .unwrap()
    }
    None
}

fn print_analyze_errors(source: &str, filename: &'static str, errors: Vec<analyzer::Error<'_>>) {
    for err in &errors {
        let stage = "analysis";
        Report::build(ReportKind::Error, (filename, err.span.into_range()))
            .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
            .with_message(format!("during {stage}, {}", err.message))
            .with_label(
                Label::new((filename, err.span.into_range()))
                    .with_message(&err.message)
                    .with_color(Color::Red),
            )
            .finish()
            .eprint(sources([(filename, &source)]))
            .unwrap()
    }
}
