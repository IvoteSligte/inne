use std::ffi::OsStr;
use std::ops::Deref;

use std::path::{Path, PathBuf};

/// A borrowed path with Inne-specific semantics.
/// Unlike [ModuleId], this path is not guaranteed to have a parent.
#[derive(Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ModulePath(Path);

impl ModulePath {
    /// Assumes that the given path is relative to the project root
    /// and that all segments are valid identifiers.
    pub fn new(path: &Path) -> &Self {
        unsafe {
            // SAFETY: ModulePath is just a transparent wrapper around Path and thus safe to cast
            std::mem::transmute(path)
        }
    }

    /// Checks if the given path is a file path to an Inne module.
    /// Note that time-of-check-time-of-use errors may still occur after this check.
    pub fn is_module_file(path: &Path) -> bool {
        path.is_file() && path.extension() == Some(OsStr::new("inne"))
    }

    pub fn parent(&self) -> Option<&Self> {
        self.0.parent().map(Self::new)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|os_str| os_str.to_str().unwrap())
    }
}

impl std::fmt::Display for ModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, segment) in self.iter().enumerate() {
            if i > 0 {
                f.write_str("::")?;
            }
            f.write_str(segment)?;
        }
        Ok(())
    }
}

impl ToOwned for ModulePath {
    type Owned = ModulePathBuf;

    fn to_owned(&self) -> Self::Owned {
        ModulePathBuf::new(self.0.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePathBuf(PathBuf);

impl ModulePathBuf {
    pub fn new(path: PathBuf) -> Self {
        debug_assert!(path.extension().is_none());
        debug_assert!(
            path.iter()
                .all(|component| { component.to_str().is_some_and(is_identifier) })
        );
        Self(path)
    }

    pub fn push(&mut self, path: &ModulePath) {
        self.0.push(&path.0);
    }
}

impl std::ops::Deref for ModulePathBuf {
    type Target = ModulePath;

    fn deref(&self) -> &Self::Target {
        ModulePath::new(&self.0)
    }
}

impl std::fmt::Display for ModulePathBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

impl std::borrow::Borrow<ModulePath> for ModulePathBuf {
    fn borrow(&self) -> &ModulePath {
        self.deref()
    }
}

/// The unique identifier of a module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleId(PathBuf);

impl ModuleId {
    /// Assumes that the given path is relative to the project root,
    /// that all segments are valid identifiers, and that the file extension is `.inne`
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        debug_assert!(path.as_os_str().to_str().is_some());
        assert!(
            path.extension() == Some(OsStr::new("inne")),
            "Module path does not end in '.inne': '{}'",
            path.display()
        );
        Self(path.into())
    }

    pub fn root() -> Self {
        // TODO: make this not allocate
        Self::new("main.inne")
    }

    pub fn parent(&self) -> &ModulePath {
        self.deref().parent().unwrap()
    }

    /// Returns the path to the `.inne` file for this module.
    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    /// Returns the path to the `.inne` file for this module.
    pub fn as_path_str(&self) -> &str {
        self.0.as_path().as_os_str().to_str().unwrap()
    }
}

impl std::ops::Deref for ModuleId {
    type Target = ModulePath;

    fn deref(&self) -> &Self::Target {
        let path = self.0.as_path();
        let file_stem_end = path
            .file_stem()
            .unwrap()
            .as_encoded_bytes()
            .as_ptr_range()
            .end;
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let bytes = &path_bytes[0..file_stem_end
            .addr()
            .wrapping_sub(path_bytes.as_ptr().addr())];
        let os_str = unsafe {
            // SAFETY: 'bytes' starts at a valid OsStr address and ends right before a valid UTF-8 boundary.
            OsStr::from_encoded_bytes_unchecked(bytes)
        };
        ModulePath::new(Path::new(os_str))
    }
}

impl std::fmt::Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

fn is_identifier(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_alphabetic())
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}
