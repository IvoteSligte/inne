use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub struct Project {
    pub dir_path: PathBuf,
    pub root: Module,
    pub siblings: Vec<Module>,
}

impl Project {
    /// `root` must be a `.inne` file.
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = root.as_ref();
        let Some(dir_path) = path.parent() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "The project root is not a valid file path.",
            ));
        };
        let mut folder = Folder::new(dir_path)?;
        let Some(root_index) = folder
            .modules
            .iter()
            .position(|module| module.path.file_name().unwrap() == path.as_os_str())
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "The project root is not a .inne file.",
            ));
        };
        let root_module = folder.modules.remove(root_index);
        Ok(Self {
            dir_path: dir_path.to_path_buf(),
            root: root_module,
            siblings: folder.modules,
        })
    }
}

pub struct Module {
    pub path: PathBuf,
    pub source: String,
    pub folder: Option<Folder>,
}

impl Module {
    /// Returns an error if `root` is not a `.inne` file or if there is an error reading the files.
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = root.as_ref();
        if path.extension() != Some(OsStr::new("inne")) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidFilename,
                "The provided root is not a .inne file.",
            ));
        }
        let source = std::fs::read_to_string(path)?;
        let folder_path = path.parent().unwrap().join(path.file_stem().unwrap());
        let folder = match Folder::new(folder_path) {
            Ok(folder) => Some(folder),
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    || err.kind() == std::io::ErrorKind::NotADirectory =>
            {
                None
            }
            Err(err) => return Err(err),
        };
        Ok(Self {
            path: path.to_path_buf(),
            source,
            folder,
        })
    }
}

pub struct Folder {
    pub path: PathBuf,
    pub modules: Vec<Module>,
}

impl Folder {
    pub fn new(folder: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = folder.as_ref();
        debug_assert!(
            path.is_dir(),
            "The path provided to 'FileTree::new_folder' must be the path to a folder."
        );
        let modules = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.is_file()
                    && path.file_name() != Some(OsStr::new(""))
                    && path.extension() == Some(OsStr::new(".inne")))
                .then_some(path)
            })
            .map(Module::new)
            .collect::<std::io::Result<Vec<_>>>()?;
        Ok(Self {
            path: path.to_path_buf(),
            modules,
        })
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
