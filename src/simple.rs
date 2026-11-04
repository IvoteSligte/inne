//! Every function in this module returns `None` on failure.
//! Errors are logged directly to stderr.

use std::path::{Path, PathBuf};

use parser::{Ast, SpannedToken};

use crate::{Error, Hir, Mir, ModuleId, TypedHir, project};

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

pub fn read_modules_in_project(root_path: &Path) -> Option<(&Path, Vec<(ModuleId, String)>)> {
    match project::read_modules(&root_path) {
        Ok(out) => Some(out),
        Err(err) => {
            eprintln!("Internal error while reading modules: {}", err);
            return None;
        }
    }
}

pub fn lex_module<'src>(id: &'src ModuleId, source: &'src str) -> Option<Vec<SpannedToken<'src>>> {
    let (maybe_tokens, errors) = crate::lex(&source, id.as_path_str());
    eprint_errors(&errors);
    maybe_tokens
}

pub fn parse_tokens<'src>(tokens: &[SpannedToken<'src>]) -> Option<Ast<'src>> {
    let (maybe_ast, errors) = crate::parse(&tokens);
    eprint_errors(&errors);
    maybe_ast
}

pub fn hir_from_asts<'ast, 'src: 'ast>(
    asts: impl IntoIterator<Item = (&'src ModuleId, &'src str, &'ast Ast<'src>)>,
) -> Option<Hir<'src>> {
    match crate::hir_from_ast_modules(asts) {
        Ok(hir) => Some(hir),
        Err(errors) => {
            eprint_errors(&errors);
            None
        }
    }
}

pub fn typed_hir_from_hir<'src>(hir: Hir<'src>) -> Option<TypedHir<'src>> {
    match crate::typed_hir_from_hir(hir) {
        Ok(typed_hir) => Some(typed_hir),
        Err(errors) => {
            eprint_errors(&errors);
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
            eprint_errors(&errors);
            None
        }
    }
}

pub fn eprint_errors(errors: &[Error<'_>]) {
    for error in errors {
        error.eprint();
    }
}
