use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub struct Module {
    pub path: PathBuf,
    pub source: String,
    pub children: Vec<Module>,
}

impl Module {
    /// Returns an error if `root` is not a `.inne` file or if there is an error reading the files.
    pub fn new(file_path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = file_path.as_ref();
        if path.extension() != Some(OsStr::new("inne")) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidFilename,
                "The provided root is not a .inne file.",
            ));
        }
        let source = std::fs::read_to_string(path)?;
        let folder_path = path.parent().unwrap().join(path.file_stem().unwrap());
        let children = match Self::new_folder(folder_path) {
            Ok(children) => children,
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    || err.kind() == std::io::ErrorKind::NotADirectory =>
            {
                Vec::new()
            }
            Err(err) => return Err(err),
        };
        Ok(Self {
            path: path.to_path_buf(),
            source,
            children,
        })
    }

    pub fn new_folder(folder_path: impl AsRef<Path>) -> std::io::Result<Vec<Self>> {
        std::fs::read_dir(folder_path)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.is_file()
                    && path.file_name() != Some(OsStr::new(""))
                    && path.extension() == Some(OsStr::new(".inne")))
                .then_some(path)
            })
            .map(Module::new)
            .collect::<std::io::Result<Vec<_>>>()
    }

    /// `root_file_path` must be the path to a `.inne` file.
    pub fn new_project(root_file_path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = root_file_path.as_ref();
        let Some(dir_path) = path.parent() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "The project root is not a valid file path.",
            ));
        };
        let mut files = Module::new_folder(dir_path)?;
        let Some(root_index) = files
            .iter()
            .position(|module| module.path.file_name().unwrap() == path.as_os_str())
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "The project root is not a .inne file.",
            ));
        };
        let mut root_module = files.remove(root_index);
        root_module.children.extend(files);
        Ok(root_module)
    }
}

/// Tries to find a file named `main.inne` in the current directory and parent directories.
pub fn find_root() -> std::io::Result<Option<PathBuf>> {
    let current_dir = std::env::current_dir()?;
    let mut dir = current_dir.as_path();
    loop {
        for result in std::fs::read_dir(dir)? {
            if let Ok(entry) = result {
                if &entry.file_name() == OsStr::new("main.inne") {
                    return Ok(Some(entry.path()));
                }
            }
        }
        let Some(parent_dir) = dir.parent() else {
            return Ok(None);
        };
        dir = parent_dir;
    }
}
