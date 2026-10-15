use std::{ffi::OsStr, io, path::PathBuf};

pub mod ast;
pub mod hir;
pub mod interpreter;
pub mod lexer;
pub mod mir;
pub mod parser;
pub mod typed_hir;

use ariadne::{Color, Label, Report, ReportKind, sources};
use chumsky::{Parser as _, error::Rich, input::Input};
use clap::Parser;

pub use ast::{Ast, ast_from_parse_tree};
pub use hir::{Hir, hir_from_ast};
pub use interpreter::Interpreter;
pub use lexer::{Span, Spanned, Token, lexer};
pub use mir::{Mir, mir_from_typed_hir};
pub use parser::{ParseTree, parser};
pub use typed_hir::{TypedHir, typed_hir_from_hir};

#[derive(clap::Parser)]
#[command(name = "Inne")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Compiler for the Inne programming language", long_about = None)]
struct Args {
    /// The name of the Inne file to run.
    file: PathBuf,
    /// The IR to emit for compiler debugging purposes.
    #[arg(long)]
    emit: Option<Ir>,
}

#[derive(clap::ValueEnum, Clone, PartialEq, Eq)]
enum Ir {
    Tokens,
    ParseTree,
    Ast,
    Hir,
    TypedHir,
    Mir,
}

// TODO: display general diagnostics through ariadne as well
// TODO: do not immediately return from main() when any errors are encountered

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
    let tokens = match lexer().parse(&source).into_result() {
        Ok(tokens) => tokens,
        Err(errors) => {
            print_parse_errors(errors, filename, &source, "lexing");
            return;
        }
    };
    if args.emit == Some(Ir::Tokens) {
        todo!();
    }
    let parse_tree = match parser(filename)
        .parse(
            tokens
                .as_slice()
                .map((source.len()..source.len()).into(), |(t, s)| (t, s)),
        )
        .into_result()
    {
        Ok(parse_tree) => parse_tree,
        Err(errors) => {
            print_parse_errors(errors, filename, &source, "parsing");
            return;
        }
    };
    if args.emit == Some(Ir::ParseTree) {
        todo!();
    }
    let ast = ast_from_parse_tree(&parse_tree);
    if args.emit == Some(Ir::Ast) {
        todo!();
    }
    let hir = match hir_from_ast(&ast) {
        Ok(hir) => hir,
        Err(errors) => {
            print_ir_errors(&errors, &source, filename, "symbol resolution");
            return;
        }
    };
    if args.emit == Some(Ir::Hir) {
        println!("{}", hir);
        return;
    }
    let typed_hir = match typed_hir_from_hir(hir) {
        Ok(typed_hir) => typed_hir,
        Err(errors) => {
            print_ir_errors(&errors, &source, filename, "type checking");
            return;
        }
    };
    if args.emit == Some(Ir::TypedHir) {
        todo!();
    }
    let mir = match mir_from_typed_hir(&typed_hir) {
        Ok(mir) => mir,
        Err(errors) => {
            print_ir_errors(&errors, &source, filename, "control-flow analysis");
            return;
        }
    };
    if args.emit == Some(Ir::Mir) {
        println!("{}", mir);
        return;
    }
    Interpreter::run(&mir);
}

fn print_parse_errors<E>(
    errors: Vec<Rich<'_, E>>,
    filename: &'static str,
    source: &str,
    stage: &'static str,
) where
    E: std::fmt::Display,
{
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
}

fn print_ir_errors<'a>(
    errors: impl IntoIterator<Item = &'a hir::Error<'a>>,
    source: &str,
    filename: &'static str,
    stage: &str,
) {
    for err in errors {
        Report::build(ReportKind::Error, (filename, err.span.into_range()))
            .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
            .with_message(format!("during {stage}, {}", err.message))
            .with_label(
                Label::new((filename, err.span.into_range()))
                    .with_message(&err.message)
                    .with_color(Color::Red),
            )
            .with_labels(err.contexts.iter().map(|(message, span)| {
                Label::new((filename, span.into_range()))
                    .with_message(message)
                    .with_color(Color::Yellow)
            }))
            .finish()
            .eprint(sources([(filename, &source)]))
            .unwrap()
    }
}
