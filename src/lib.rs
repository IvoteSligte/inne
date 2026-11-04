pub mod hir;
pub mod interpreter;
pub mod mir;
pub mod path;
pub mod project;
pub mod simple;
pub mod typed_hir;
mod util;
pub mod value;

use std::path::PathBuf;

pub use error::Errors;
pub use error::*;
pub use hir::{Hir, hir_from_ast_modules};
pub use interpreter::Interpreter;
pub use mir::{Mir, mir_from_typed_hir};
pub use parser::{self, Ast, LexError, ParseError, Span, Spanned, Token, ast};
pub use path::*;
pub use typed_hir::{TypedHir, typed_hir_from_hir};
pub use value::Value;

pub fn lex<'src>(
    source: &'src str,
    filename: &'src str,
) -> (Option<Vec<(Token<'src>, Span<'src>)>>, Vec<Error<'src>>) {
    let (tokens, errors) = parser::lex(source, filename);
    let errors = errors
        .into_iter()
        .map(Error::from_lex_error)
        .collect::<Vec<_>>();
    (errors.is_empty().then_some(tokens), errors)
}

pub fn parse<'src>(tokens: &[(Token<'src>, Span<'src>)]) -> (Option<Ast<'src>>, Vec<Error<'src>>) {
    let (ast, errors) = parser::parse(tokens);
    let errors = errors
        .into_iter()
        .map(Error::from_parse_error)
        .collect::<Vec<_>>();
    (errors.is_empty().then_some(ast), errors)
}

/// `root_path` is an optional path to the project root (i.e. `main.inne` file).
/// A root file is searched for relative to the current working directory if `root_path` is `None`.
/// Returns `true` on success.
pub fn compile_and_run(root_path: Option<PathBuf>) -> bool {
    use simple::*;

    (|| {
        debug_assert!(root_path.as_ref().is_none_or(|path| path.is_file()));
        let root_path = root_path.or_else(find_project_root)?;
        let files = read_modules_in_project(&root_path)?;
        let mut asts = Vec::new();
        for (id, source) in &files {
            let tokens = lex_module(id, source)?;
            let ast = parse_tokens(&tokens)?;
            asts.push((id, source, ast));
        }
        let ast_modules = asts
            .iter()
            .map(|(path, source, ast)| (*path, source.as_str(), ast));
        let hir = hir_from_asts(ast_modules)?;
        let typed_hir = typed_hir_from_hir(hir)?;
        let mir = mir_from_typed_hir(&typed_hir)?;
        Interpreter::run(&mir);
        Some(())
    })()
    .is_some()
}
