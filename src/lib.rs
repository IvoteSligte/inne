pub mod hir;
pub mod interpreter;
pub mod mir;
pub mod project;
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

pub mod simple {
    //! Every function in this module returns `None` on failure.
    //! Errors are logged directly to stderr.

    use std::path::{Path, PathBuf};

    use parser::{Ast, SpannedToken};

    use crate::{Error, Hir, Mir, TypedHir, project};

    pub fn find_project_root() -> Option<PathBuf> {
        match project::find_root(None) {
            Ok(Some(path)) => Some(path),
            Ok(None) => {
                eprintln!(
                    "Could not find file 'main.inne' in the current directory or in a parent directory."
                );
                None
            }
            Err(err) => {
                eprintln!("Internal error while searching for 'main.inne': {}", err);
                None
            }
        }
    }

    pub fn read_files_in_project(root_path: &Path) -> Option<Vec<(PathBuf, String)>> {
        match project::read_files(&root_path) {
            Ok(files) => Some(files),
            Err(err) => {
                eprintln!("Internal error while reading modules: {}", err);
                return None;
            }
        }
    }

    pub fn lex_file<'src>(path: &'src Path, source: &'src str) -> Option<Vec<SpannedToken<'src>>> {
        let filename = path.as_os_str().to_str().unwrap();
        let (maybe_tokens, errors) = crate::lex(&source, filename);
        eprint_errors(&errors, "lexing");
        maybe_tokens
    }

    pub fn parse_tokens<'src>(tokens: &[SpannedToken<'src>]) -> Option<Ast<'src>> {
        let (maybe_ast, errors) = crate::parse(&tokens);
        eprint_errors(&errors, "parsing");
        maybe_ast
    }

    pub fn hir_from_asts<'ast, 'src: 'ast>(
        asts: impl IntoIterator<Item = (&'src Path, &'src str, &'ast Ast<'src>)>,
        root_path: &'src Path,
    ) -> Option<Hir<'src>> {
        match crate::hir_from_ast_modules(asts, root_path) {
            Ok(hir) => Some(hir),
            Err(errors) => {
                eprint_errors(&errors, "symbol resolution");
                None
            }
        }
    }

    pub fn typed_hir_from_hir<'src>(hir: Hir<'src>) -> Option<TypedHir<'src>> {
        match crate::typed_hir_from_hir(hir) {
            Ok(typed_hir) => Some(typed_hir),
            Err(errors) => {
                eprint_errors(&errors, "type checking");
                None
            }
        }
    }

    pub fn mir_from_typed_hir<'typed_hir, 'src: 'typed_hir>(
        typed_hir: &'typed_hir TypedHir<'src>,
    ) -> Option<Mir<'src>> {
        match crate::mir_from_typed_hir(typed_hir) {
            Ok(mir) => Some(mir),
            Err(errors) => {
                eprint_errors(&errors, "control-flow analysis");
                None
            }
        }
    }

    pub fn eprint_errors(errors: &[Error<'_>], stage: &str) {
        for error in errors {
            error.eprint(stage);
        }
    }
}

/// `root_path` is an optional path to the project root (i.e. `main.inne` file).
/// A root file is searched for relative to the current working directory if `root_path` is `None`.
/// Returns `true` on success.
pub fn compile_and_run(root_path: Option<PathBuf>) -> bool {
    use simple::*;

    (|| {
        debug_assert!(root_path.as_ref().is_none_or(|path| path.is_file()));
        let root_path = root_path.or_else(find_project_root)?;
        let files = read_files_in_project(&root_path)?;
        let mut asts = Vec::new();
        for (path, source) in &files {
            let tokens = lex_file(path, source)?;
            let ast = parse_tokens(&tokens)?;
            asts.push((path, source, ast));
        }
        let ast_modules = asts
            .iter()
            .map(|(path, source, ast)| (path.as_path(), source.as_str(), ast));
        let hir = hir_from_asts(ast_modules, &root_path)?;
        let typed_hir = typed_hir_from_hir(hir)?;
        let mir = mir_from_typed_hir(&typed_hir)?;
        Interpreter::run(&mir);
        Some(())
    })()
    .is_some()
}
