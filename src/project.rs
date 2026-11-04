use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use crate::{ModuleId, ModulePath};

fn directory_containing(path: &Path) -> std::io::Result<&Path> {
    if path.is_dir() {
        return Ok(path);
    }
    if path.is_file() {
        return Ok(path.parent().unwrap());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Not a file or directory path: '{}'", path.display()),
    ))
}

/// Reads `.inne` files in the same directory as `relative_to`.
/// `relative_to` may also be a directory path itself.
pub fn read_modules(relative_to: impl AsRef<Path>) -> std::io::Result<Vec<(ModuleId, String)>> {
    let path = relative_to.as_ref();
    let folder_path = directory_containing(path)?;
    // TODO: nested folders
    let mut files = Vec::new();

    for result in std::fs::read_dir(folder_path)? {
        let path = result?.path();
        if ModulePath::is_module_file(&path) {
            let source = std::fs::read_to_string(&path)?;
            let id = ModuleId::new(path.strip_prefix(folder_path).unwrap());
            files.push((id, source));
        }
    }
    Ok(files)
}

/// Tries to find a file named `main.inne` in the current directory and parent directories.
/// If `relative_to` is `Some`, then the directory containing `relative_to` is used as starting point.
/// Returns `Ok(None)` if it cannot be found, otherwise the canonical path to the file.
pub fn find_root(relative_to: Option<PathBuf>) -> std::io::Result<Option<PathBuf>> {
    let relative_to = match relative_to {
        Some(path) => directory_containing(&path)?.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let mut dir = relative_to.as_path();
    loop {
        for result in std::fs::read_dir(dir)? {
            if let Ok(entry) = result {
                if &entry.file_name() == OsStr::new("main.inne") && entry.metadata()?.is_file() {
                    return Ok(Some(entry.path().canonicalize()?));
                }
            }
        }
        let Some(parent_dir) = dir.parent() else {
            return Ok(None);
        };
        dir = parent_dir;
    }
}
