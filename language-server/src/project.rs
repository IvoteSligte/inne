use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use inne::{Error, ModuleId};
use tower_lsp_server::ls_types::*;

pub struct Project {
    pub files: Vec<File>,
}

impl Project {
    /// Returns `None` on error.
    pub fn new(root_path: &Path) -> Option<Self> {
        let modules = match inne::project::read_modules(&root_path) {
            Ok(modules) => modules,
            Err(err) => {
                eprintln!("Failed to read project modules: {}", err);
                return None;
            }
        };
        Some(Self {
            files: modules
                .into_iter()
                .map(|(id, source)| {
                    // NOTE: unwrap can theoretically fail if the file was removed since the last check
                    let uri = Uri::from_file_path(id.as_path()).unwrap();
                    File::new(&uri, source, i32::MIN)
                })
                .collect::<Option<Vec<_>>>()?,
        })
    }

    // FIXME: accessing by URI is ambiguous if relative URIs are allowed, so maybe prefer absolute paths?
    pub fn get_file(&self, uri: &Uri) -> Option<&File> {
        self.files.iter().find(|file| &file.uri == uri)
    }

    pub fn get_file_mut(&mut self, uri: &Uri) -> Option<&mut File> {
        self.files.iter_mut().find(|file| &file.uri == uri)
    }

    pub fn analyze(&self) -> Vec<Error<'_>> {
        let mut all_errors = Vec::new();
        let mut asts = Vec::new();
        for file in &self.files {
            eprintln!("Lexing file '{}'", file.name);
            let (maybe_tokens, errors) = inne::lex(&file.source, &file.name);
            all_errors.extend(errors);
            let Some(tokens) = maybe_tokens else {
                continue;
            };
            eprintln!("Parsing file '{}'", file.name);
            let (maybe_ast, errors) = inne::parse(&tokens);
            all_errors.extend(errors);
            let Some(ast) = maybe_ast else {
                continue;
            };
            asts.push((file, ast));
        }
        if !all_errors.is_empty() {
            return all_errors;
        }
        eprintln!("Converting ASTs to HIR");
        let asts = asts
            .iter()
            .map(|(file, ast)| (&file.id, file.source.as_str(), ast));
        let hir = match inne::hir_from_ast_modules(asts) {
            Ok(hir) => hir,
            Err(errors) => {
                all_errors.extend(errors);
                return all_errors;
            }
        };
        eprintln!("Converting HIR to Typed HIR");
        let _typed_hir = match inne::typed_hir_from_hir(hir) {
            Ok(typed_hir) => typed_hir,
            Err(errors) => {
                all_errors.extend(errors);
                return all_errors;
            }
        };
        eprintln!("Analyzing project yielded {} errors", all_errors.len());
        // NOTE: THIR -> MIR currently does not produce diagnostics, so it is skipped
        all_errors
    }
}

/// Finds the root of a project, given the path to a file in it.
pub fn find_root(path_to_file: PathBuf) -> Option<PathBuf> {
    match inne::project::find_root(Some(path_to_file)) {
        // NOTE: unwrap can theoretically fail if the file was removed since the last check
        Ok(Some(path)) => Some(path.canonicalize().unwrap()),
        Ok(None) => {
            // TODO: show this message to the user
            eprintln!("Cannot find project root ('main.inne')");
            None
        }
        Err(err) => {
            eprintln!(
                "Error while searching for project root ('main.inne'): {}",
                err
            );
            None
        }
    }
}

pub struct File {
    pub version: Option<i32>,
    pub uri: Uri,
    /// Human-readable filename.
    pub name: String,
    /// Unique identifier of the module.
    pub id: ModuleId,
    /// Source-code contained in the file.
    pub source: String,
    /// The last-computed list of semantic tokens and its `result_id`.
    pub semantic_tokens: HashMap<String, (Instant, Vec<SemanticToken>)>,
}

impl File {
    pub fn new(uri: &Uri, source: impl ToString, version: i32) -> Option<Self> {
        let source = source.to_string();
        let name = uri.as_str().to_string(); // TODO: better human-readable name
        let Some(path) = uri_to_path(uri) else {
            eprintln!("The provided URI is not a file: '{}'", uri.as_str());
            return None;
        };
        Some(Self {
            version: Some(version),
            uri: uri.clone(),
            name,
            id: ModuleId::new(path),
            source,
            semantic_tokens: HashMap::new(),
        })
    }
}

pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    uri.to_file_path().as_deref().map(Path::to_path_buf)
}

pub fn get_file<'p>(projects: &'p [Project], uri: &Uri) -> Option<&'p File> {
    projects.iter().find_map(|project| project.get_file(uri))
}
