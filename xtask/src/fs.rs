//! Filesystem port for safe reconciliation.

use std::path::Path;

use crate::XtaskError;

/// Filesystem operations needed by bootstrap reconciliation.
pub trait WorkspaceFileSystem {
    /// Reads UTF-8 content when a file exists.
    ///
    /// # Errors
    ///
    /// Returns [`XtaskError`] when metadata, bytes, or UTF-8 decoding cannot be read.
    fn read(&self, path: &Path) -> Result<Option<String>, XtaskError>;
    /// Atomically writes one complete file.
    ///
    /// # Errors
    ///
    /// Returns [`XtaskError`] when directory creation, temporary writing, syncing, or renaming
    /// fails.
    fn write_atomic(&self, path: &Path, content: &str) -> Result<(), XtaskError>;
}

/// Host filesystem implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostFileSystem;

impl WorkspaceFileSystem for HostFileSystem {
    fn read(&self, path: &Path) -> Result<Option<String>, XtaskError> {
        match std::fs::read_to_string(path) {
            Ok(content) => Ok(Some(content)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(XtaskError::Bootstrap(format!(
                "{}: {error}",
                path.display()
            ))),
        }
    }

    fn write_atomic(&self, path: &Path, content: &str) -> Result<(), XtaskError> {
        let parent = path
            .parent()
            .ok_or_else(|| XtaskError::Bootstrap("path has no parent".into()))?;
        std::fs::create_dir_all(parent)
            .map_err(|error| XtaskError::Bootstrap(error.to_string()))?;
        let temporary = path.with_extension("xtask.tmp");
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&temporary)
                .map_err(|error| XtaskError::Bootstrap(error.to_string()))?;
            file.write_all(content.as_bytes())
                .map_err(|error| XtaskError::Bootstrap(error.to_string()))?;
            file.sync_all()
                .map_err(|error| XtaskError::Bootstrap(error.to_string()))?;
        }
        std::fs::rename(&temporary, path).map_err(|error| XtaskError::Bootstrap(error.to_string()))
    }
}
