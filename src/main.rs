use std::{
    ffi::OsStr,
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
    file: Option<PathBuf>,
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

fn main() {
    let args = Args::parse();
    let root_path = match args.file.clone() {
        Some(path) if !path.exists() => {
            eprintln!("The file '{}' does not exist.", path.display());
            return;
        }
        Some(path) if !path.is_file() => {
            eprintln!(
                "The file to be compiled '{}' is not a file.",
                path.display()
            );
            return;
        }
        Some(path) => {
            if path.extension() != Some(OsStr::new("inne")) {
                eprintln!(
                    "Warning: The file '{}' does not seem to be a '.inne' file. Will try to run it anyway.",
                    path.display()
                );
            }
            path
        }
        None => match project::find_root(None) {
            Ok(Some(path)) => path,
            Ok(None) => {
                eprintln!(
                    "Could not find file 'main.inne' in the current directory or in a parent directory."
                );
                return;
            }
            Err(err) => {
                eprintln!("Internal error while searching for 'main.inne': {}", err);
                return;
            }
        },
    };
    let modules = match project::read_modules(&root_path) {
        Ok(modules) => modules,
        Err(err) => {
            eprintln!("Internal error while reading modules: {}", err);
            return;
        }
    };
    let mut module_asts = Vec::new();
    for (path, source) in &modules {
        let Some(module_ast) = lex_and_parse(&args, path, source) else {
            return;
        };
        module_asts.push(module_ast);
    }
    // NOTE: not sure if `path == root_path` is valid when they might be relative to different directories...
    //       maybe use Path::canonicalize?
    let Some(root_index) = module_asts
        .iter()
        .position(|(path, _, _)| *path == root_path)
    else {
        eprintln!("Internal error: could not find the root module in the read modules.");
        return;
    };
    let module_asts = module_asts
        .iter()
        .map(|(path, source, ast)| (*path, *source, ast));
    let hir = match hir_from_ast_modules(module_asts, root_index) {
        Ok(hir) => hir,
        Err(errors) => {
            eprint_errors(&errors, "symbol resolution");
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
            eprint_errors(&errors, "type checking");
            return;
        }
    };
    if args.emit == Some(Ir::TypedHir) {
        todo!();
    }
    let mir = match mir_from_typed_hir(&typed_hir) {
        Ok(mir) => mir,
        Err(errors) => {
            eprint_errors(&errors, "control-flow analysis");
            return;
        }
    };
    if args.emit == Some(Ir::Mir) {
        println!("{}", mir);
        return;
    }
    Interpreter::run(&mir);
}

fn lex_and_parse<'src>(
    args: &Args,
    path: &'src Path,
    source: &'src str,
) -> Option<(&'src Path, &'src str, Ast<'src>)> {
    let filename = path.as_os_str().to_str().unwrap();
    let (maybe_tokens, errors) = lex(&source, filename);
    eprint_errors(&errors, "lexing");
    let Some(tokens) = maybe_tokens else {
        return None;
    };
    if args.emit == Some(Ir::Tokens) {
        eprintln!("Tokens for file '{filename}':");
        for (token, span) in &tokens {
            println!("{:<30} at {span}", format!("{:?}", token));
        }
        return None;
    }
    let (maybe_ast, errors) = parse(&tokens, &source);
    eprint_errors(&errors, "parsing");
    let Some(ast) = maybe_ast else {
        return None;
    };
    if args.emit == Some(Ir::Ast) {
        todo!();
    }
    Some((path, source, ast))
}

pub fn eprint_errors(errors: &[Error], stage: &str) {
    for error in errors {
        error.eprint(stage);
    }
}
