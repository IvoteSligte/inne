use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

use clap::Parser;
use inne::*;

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
    // TODO: fmt command
}

#[derive(clap::ValueEnum, Clone, PartialEq, Eq)]
enum Ir {
    Tokens,
    Ast,
    Hir,
    TypedHir,
    Mir,
}

// TODO: display general diagnostics through ariadne as well
// TODO: do not immediately return from main() when any errors are encountered

fn read_file(path: impl AsRef<Path>, filename: &str) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(source) => Some(source),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            eprintln!("Error: The file '{}' does not exist.", filename);
            None
        }
        Err(err) if err.kind() == io::ErrorKind::IsADirectory => {
            eprintln!("Error: '{}' is a directory, not a '.inne' file.", filename);
            None
        }
        Err(err) => {
            eprintln!("Error: Failed to read file '{}': {}", filename, err);
            None
        }
    }
}

fn main() {
    let args = Args::parse();
    // Human-readable filename.
    let filename = args.file.as_os_str().to_string_lossy();
    let Some(source) = read_file(&args.file, &filename) else {
        return;
    };
    if args.file.extension() != Some(OsStr::new("inne")) {
        eprintln!(
            "Warning: The file '{}' does not seem to be a '.inne' file. Will try to compile it anyway.",
            filename
        );
    }
    let filename = format!("{}", args.file.display());
    let (maybe_tokens, errors) = lex(&source);
    eprint_errors(&errors, &source, &filename, "lexing");
    let Some(tokens) = maybe_tokens else {
        return;
    };
    if args.emit == Some(Ir::Tokens) {
        eprintln!("Tokens for file '{filename}':");
        for (token, span) in &tokens {
            println!("{:<30} at {span}", format!("{:?}", token));
        }
        return;
    }
    let (maybe_ast, errors) = parse(&tokens, &source, &filename);
    eprint_errors(&errors, &source, &filename, "parsing");
    let Some(ast) = maybe_ast else {
        return;
    };
    if args.emit == Some(Ir::Ast) {
        todo!();
    }
    let hir = match hir_from_ast(&ast) {
        Ok(hir) => hir,
        Err(errors) => {
            eprint_errors(&errors, &source, &filename, "symbol resolution");
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
            eprint_errors(&errors, &source, &filename, "type checking");
            return;
        }
    };
    if args.emit == Some(Ir::TypedHir) {
        todo!();
    }
    let mir = match mir_from_typed_hir(&typed_hir) {
        Ok(mir) => mir,
        Err(errors) => {
            eprint_errors(&errors, &source, &filename, "control-flow analysis");
            return;
        }
    };
    if args.emit == Some(Ir::Mir) {
        println!("{}", mir);
        return;
    }
    Interpreter::run(&mir);
}

pub fn eprint_errors(errors: &[Error], source: &str, filename: &str, stage: &str) {
    for error in errors {
        error.eprint(source, filename, stage);
    }
}
