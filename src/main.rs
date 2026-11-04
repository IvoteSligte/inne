use std::process::exit;

use clap::Parser;
use inne::*;

#[derive(clap::Parser)]
#[command(name = "Inne")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Compiler for the Inne programming language", long_about = None)]
struct Args {
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

    use inne::simple::*;

    match (|| {
        let root_path = find_project_root()?;
        let (_, files) = read_modules_in_project(&root_path)?;
        let mut asts = Vec::new();
        for (id, source) in &files {
            let tokens = lex_module(id, source)?;
            if args.emit == Some(Ir::Tokens) {
                eprintln!("Tokens for file '{}':", id);
                for (token, span) in &tokens {
                    println!("{:<30} at {span}", format!("{:?}", token));
                }
                return Some(());
            }
            let ast = parse_tokens(&tokens)?;
            if args.emit == Some(Ir::Ast) {
                todo!();
            }
            asts.push((id, source, ast));
        }
        let ast_modules = asts
            .iter()
            .map(|(id, source, ast)| (*id, source.as_str(), ast));
        let hir = hir_from_asts(ast_modules)?;
        if args.emit == Some(Ir::Hir) {
            println!("{}", hir);
            return Some(());
        }
        let typed_hir = typed_hir_from_hir(hir)?;
        if args.emit == Some(Ir::TypedHir) {
            todo!();
        }
        let mir = mir_from_typed_hir(&typed_hir)?;
        if args.emit == Some(Ir::Hir) {
            println!("{}", mir);
            return Some(());
        }
        Interpreter::run(&mir);
        Some(())
    })() {
        Some(()) => exit(0),
        None => exit(1),
    }
}
