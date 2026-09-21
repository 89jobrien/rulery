//! Filesystem-backed package loading.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rulery_contracts::{
    BoundaryError, LoadedSourceBundle, PackageId, PackagePath, RulebookLock, RulebookLockEnvelope,
    SourceBundle, SourceDocument, SourcePath, StableId, VersionRequirement,
};
use thiserror::Error;

use crate::integrity::IntegrityCalculator;

const REQUIRED_ROOT_FILES: [&str; 3] = ["rulery.yaml", "vocabulary.yaml", "actions.yaml"];
const REQUIRED_DIRS: [&str; 2] = ["rules", "scenarios"];
const LOCK_FILE_NAME: &str = "rulery.lock";

/// Import declaration used by package assembly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportRef {
    /// Declared package identity.
    pub package: PackageId,
    /// Declared version requirement.
    pub version: VersionRequirement,
    /// Relative local filesystem path to the import target.
    pub path: String,
    /// Optional local alias.
    pub alias: Option<StableId>,
}

/// Source package loader abstraction.
pub trait PackageStore {
    /// Loads the exact authored source bundle from a local package root.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when required files are missing, paths are unsafe, or bytes cannot
    /// be decoded and validated as source documents.
    fn load_source(&self, root: &PackagePath) -> Result<LoadedSourceBundle, StoreError>;

    /// Resolves and loads one local import rooted under the importer package.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the import path is absolute, empty, escapes the importer root,
    /// or cannot be loaded as a valid source bundle.
    fn resolve_import(
        &self,
        importer: &PackagePath,
        import: &ImportRef,
        lock: Option<&RulebookLock>,
    ) -> Result<LoadedSourceBundle, StoreError>;

    /// Loads a lock file when present.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the lock file exists but is not a strict v1 lock envelope.
    fn load_lock(&self, root: &PackagePath) -> Result<Option<RulebookLock>, StoreError>;

    /// Writes a lock file atomically using a temporary sibling file then rename.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when encoding or filesystem writes fail.
    fn write_lock(&self, root: &PackagePath, lock: &RulebookLock) -> Result<(), StoreError>;
}

/// Filesystem package-store implementation.
#[derive(Clone, Debug)]
pub struct FilesystemPackageStore {
    max_files: usize,
    max_bytes: u64,
    fail_before_rename: bool,
}

impl Default for FilesystemPackageStore {
    fn default() -> Self {
        Self {
            max_files: 1_024,
            max_bytes: 32 * 1024 * 1024,
            fail_before_rename: false,
        }
    }
}

impl FilesystemPackageStore {
    /// Creates a package store with explicit resource limits.
    #[must_use]
    pub const fn with_limits(max_files: usize, max_bytes: u64) -> Self {
        Self {
            max_files,
            max_bytes,
            fail_before_rename: false,
        }
    }

    #[cfg(test)]
    #[must_use]
    fn with_fail_before_rename(mut self, value: bool) -> Self {
        self.fail_before_rename = value;
        self
    }
}

impl PackageStore for FilesystemPackageStore {
    fn load_source(&self, root: &PackagePath) -> Result<LoadedSourceBundle, StoreError> {
        let root_path = root.as_ref();
        let canonical_root = canonicalize_path(root_path)?;
        if !canonical_root.is_dir() {
            return Err(StoreError::MissingRequiredDirectory {
                path: canonical_root,
            });
        }

        let mut collected = Vec::new();
        let mut total_bytes = 0_u64;

        for file in REQUIRED_ROOT_FILES {
            let full_path = canonical_root.join(file);
            if !full_path.exists() {
                return Err(StoreError::MissingRequiredFile {
                    path: full_path.clone(),
                });
            }
            let relative = SourcePath::new(file.to_owned()).map_err(StoreError::from)?;
            let document = read_document(&canonical_root, &full_path, relative)?;
            total_bytes = total_bytes.saturating_add(document.content().len() as u64);
            collected.push(document);
        }

        for directory in REQUIRED_DIRS {
            let directory_path = canonical_root.join(directory);
            if !directory_path.is_dir() {
                return Err(StoreError::MissingRequiredDirectory {
                    path: directory_path,
                });
            }

            let mut entries: Vec<PathBuf> = fs::read_dir(&directory_path)
                .map_err(|source| StoreError::Io {
                    path: directory_path.clone(),
                    source,
                })?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
                .collect();
            entries.sort_by_key(|path| path.file_name().map(std::ffi::OsStr::to_os_string));

            for entry in entries {
                let file_name = entry
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| StoreError::NonUtf8Path {
                        path: entry.clone(),
                    })?;
                let relative = SourcePath::new(format!("{directory}/{file_name}"))
                    .map_err(StoreError::from)?;
                let document = read_document(&canonical_root, &entry, relative)?;
                total_bytes = total_bytes.saturating_add(document.content().len() as u64);
                collected.push(document);
            }
        }

        if collected.len() > self.max_files {
            return Err(StoreError::ResourceLimitExceeded {
                kind: "files",
                max: self.max_files as u64,
            });
        }
        if total_bytes > self.max_bytes {
            return Err(StoreError::ResourceLimitExceeded {
                kind: "bytes",
                max: self.max_bytes,
            });
        }

        let bundle = SourceBundle::new(collected).map_err(StoreError::from)?;
        let integrity = IntegrityCalculator::bundle_integrity(&bundle);
        Ok(LoadedSourceBundle::new(bundle, integrity))
    }

    fn resolve_import(
        &self,
        importer: &PackagePath,
        import: &ImportRef,
        _lock: Option<&RulebookLock>,
    ) -> Result<LoadedSourceBundle, StoreError> {
        let relative = import.path.trim();
        if relative.is_empty() {
            return Err(StoreError::InvalidImportPath {
                path: import.path.clone(),
            });
        }
        if Path::new(relative).is_absolute() || relative.contains('\\') {
            return Err(StoreError::InvalidImportPath {
                path: import.path.clone(),
            });
        }
        let source_path =
            SourcePath::new(relative.to_owned()).map_err(|_| StoreError::InvalidImportPath {
                path: import.path.clone(),
            })?;

        let importer_root = canonicalize_path(importer.as_ref())?;
        let joined = importer_root.join(source_path.as_str());
        let resolved = canonicalize_path(&joined)?;
        if !resolved.starts_with(&importer_root) {
            return Err(StoreError::PathEscapesRoot { path: joined });
        }

        let target_root = if resolved.is_file() {
            resolved.parent().map(Path::to_path_buf).ok_or_else(|| {
                StoreError::InvalidImportPath {
                    path: import.path.clone(),
                }
            })?
        } else {
            resolved
        };
        self.load_source(&PackagePath::new(target_root).map_err(StoreError::from)?)
    }

    fn load_lock(&self, root: &PackagePath) -> Result<Option<RulebookLock>, StoreError> {
        let root = canonicalize_path(root.as_ref())?;
        let lock_path = root.join(LOCK_FILE_NAME);
        if !lock_path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&lock_path).map_err(|source| StoreError::Io {
            path: lock_path.clone(),
            source,
        })?;
        let envelope =
            serde_json::from_slice::<RulebookLockEnvelope>(&bytes).map_err(|source| {
                StoreError::InvalidLockEnvelope {
                    path: lock_path,
                    source,
                }
            })?;
        Ok(Some(RulebookLock::from(envelope)))
    }

    fn write_lock(&self, root: &PackagePath, lock: &RulebookLock) -> Result<(), StoreError> {
        let root = canonicalize_path(root.as_ref())?;
        let destination = root.join(LOCK_FILE_NAME);
        let temp_path = root.join(format!(".{LOCK_FILE_NAME}.tmp"));

        let envelope = RulebookLockEnvelope::from(lock.clone());
        let encoded = serde_json::to_vec(&envelope).map_err(StoreError::LockSerialization)?;

        {
            let mut temp = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)
                .map_err(|source| StoreError::Io {
                    path: temp_path.clone(),
                    source,
                })?;
            temp.write_all(&encoded).map_err(|source| StoreError::Io {
                path: temp_path.clone(),
                source,
            })?;
            temp.sync_all().map_err(|source| StoreError::Io {
                path: temp_path.clone(),
                source,
            })?;
        }

        if self.fail_before_rename {
            let _ = fs::remove_file(&temp_path);
            return Err(StoreError::InjectedAtomicWriteFailure { path: temp_path });
        }

        fs::rename(&temp_path, &destination).map_err(|source| StoreError::Io {
            path: destination,
            source,
        })?;
        Ok(())
    }
}

fn read_document(
    canonical_root: &Path,
    path: &Path,
    relative: SourcePath,
) -> Result<SourceDocument, StoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(StoreError::SymlinkNotAllowed {
            path: path.to_path_buf(),
        });
    }

    let canonical_file = canonicalize_path(path)?;
    if !canonical_file.starts_with(canonical_root) {
        return Err(StoreError::PathEscapesRoot {
            path: path.to_path_buf(),
        });
    }
    let bytes = fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| StoreError::NonUtf8Source {
        path: path.to_path_buf(),
    })?;
    Ok(SourceDocument::new(relative, Arc::<str>::from(text)))
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, StoreError> {
    path.canonicalize().map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Store-layer package loading error.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Filesystem call failed.
    #[error("filesystem operation failed for `{path}`: {source}")]
    Io {
        /// Path associated with the operation.
        path: PathBuf,
        /// Original filesystem error.
        source: std::io::Error,
    },
    /// A required root file is absent.
    #[error("required source file is missing: `{path}`")]
    MissingRequiredFile {
        /// Missing path.
        path: PathBuf,
    },
    /// A required directory is absent.
    #[error("required source directory is missing: `{path}`")]
    MissingRequiredDirectory {
        /// Missing path.
        path: PathBuf,
    },
    /// A path could not be represented as UTF-8.
    #[error("source path is not valid UTF-8: `{path}`")]
    NonUtf8Path {
        /// Offending path.
        path: PathBuf,
    },
    /// A source file contained non-UTF-8 bytes.
    #[error("source file is not valid UTF-8: `{path}`")]
    NonUtf8Source {
        /// Offending path.
        path: PathBuf,
    },
    /// Symlinked source files are not allowed.
    #[error("symlinked source files are not allowed: `{path}`")]
    SymlinkNotAllowed {
        /// Symlink path.
        path: PathBuf,
    },
    /// A resolved source path escaped the package root.
    #[error("source path escapes package root: `{path}`")]
    PathEscapesRoot {
        /// Offending path.
        path: PathBuf,
    },
    /// File or byte count exceeded configured limits.
    #[error("source {kind} exceeded configured maximum of {max}")]
    ResourceLimitExceeded {
        /// Limited resource kind.
        kind: &'static str,
        /// Configured maximum value.
        max: u64,
    },
    /// Boundary contract validation failed.
    #[error("boundary value rejected: {0}")]
    Boundary(#[from] BoundaryError),
    /// Import path was invalid.
    #[error("import path must be non-empty, relative, and normalized: `{path}`")]
    InvalidImportPath {
        /// Rejected import path.
        path: String,
    },
    /// Lock envelope JSON was invalid or not strict v1.
    #[error("invalid lock envelope at `{path}`: {source}")]
    InvalidLockEnvelope {
        /// Lock file path.
        path: PathBuf,
        /// JSON decode error.
        source: serde_json::Error,
    },
    /// Lock serialization failed.
    #[error("failed to serialize lock envelope: {0}")]
    LockSerialization(serde_json::Error),
    /// Test-only injected failure before lock-file rename.
    #[error("injected failure before lock rename at `{path}`")]
    InjectedAtomicWriteFailure {
        /// Temporary lock file path.
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use rulery_contracts::{
        ContentHash, LanguageVersion, LockedImport, LockedRoot, NormalizedSourceLocation,
        PackageId, PackagePath, RulebookLock, Version, VersionRequirement,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    use crate::integrity::IntegrityCalculator;

    use super::*;

    #[test]
    fn filesystem_store_selects_exact_authored_layout() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir_all(root.join("rules")).expect("rules dir");
        fs::create_dir_all(root.join("scenarios")).expect("scenarios dir");
        write_file(root.join("rulery.yaml"), "name: main\r\n");
        write_file(root.join("vocabulary.yaml"), "roots: []\n");
        write_file(root.join("actions.yaml"), "actions: []\n");
        write_file(root.join("rules/z-last.yaml"), "rules: []\n");
        write_file(root.join("rules/a-first.yaml"), "rules: []\n");
        write_file(root.join("scenarios/b-case.yaml"), "scenario: {}\n");
        write_file(root.join("notes.txt"), "ignore me\n");

        let store = FilesystemPackageStore::default();
        let loaded = store
            .load_source(&PackagePath::new(root.to_path_buf()).expect("package path"))
            .expect("load source");
        let bundle = loaded.bundle();
        let paths: Vec<_> = bundle
            .documents()
            .iter()
            .map(|document| document.path().as_str().to_owned())
            .collect();
        assert_eq!(
            paths,
            vec![
                "actions.yaml",
                "rulery.yaml",
                "rules/a-first.yaml",
                "rules/z-last.yaml",
                "scenarios/b-case.yaml",
                "vocabulary.yaml",
            ]
        );

        let rules_docs: Vec<_> = paths
            .iter()
            .filter(|path| path.starts_with("rules/"))
            .cloned()
            .collect();
        assert_eq!(rules_docs, vec!["rules/a-first.yaml", "rules/z-last.yaml"]);
        assert!(!paths.iter().any(|path| path == "notes.txt"));

        let with_crlf = ContentHash::digest("name: main\r\n".as_bytes());
        let with_lf = ContentHash::digest("name: main\n".as_bytes());
        assert_ne!(with_crlf, with_lf);
        let frame = IntegrityCalculator::bundle_frame_bytes(bundle);
        let first_doc = bundle.documents().first().expect("first doc");
        let first_path = first_doc.path().as_str().as_bytes();
        let first_content = first_doc.content().as_bytes();
        let mut expected_prefix = Vec::new();
        expected_prefix.extend_from_slice(&(first_path.len() as u64).to_be_bytes());
        expected_prefix.extend_from_slice(first_path);
        expected_prefix.extend_from_slice(&(first_content.len() as u64).to_be_bytes());
        expected_prefix.extend_from_slice(first_content);
        assert!(frame.starts_with(&expected_prefix));

        let bundle_hash = ContentHash::digest(&frame);
        assert_eq!(loaded.integrity().as_bytes(), bundle_hash.as_bytes());

        fs::remove_file(root.join("actions.yaml")).expect("remove actions");
        let missing_file = store.load_source(&PackagePath::new(root.to_path_buf()).expect("path"));
        assert!(matches!(
            missing_file,
            Err(StoreError::MissingRequiredFile { .. })
        ));

        write_file(root.join("actions.yaml"), "actions: []\n");
        fs::remove_dir_all(root.join("scenarios")).expect("remove scenarios");
        let missing_dir = store.load_source(&PackagePath::new(root.to_path_buf()).expect("path"));
        assert!(matches!(
            missing_dir,
            Err(StoreError::MissingRequiredDirectory { .. })
        ));

        fs::create_dir_all(root.join("scenarios")).expect("recreate scenarios");
        write_file(root.join("scenarios/b-case.yaml"), "scenario: {}\n");
        let outside = temp.path().join("outside.yaml");
        write_file(&outside, "external\n");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("rules/escape.yaml")).expect("symlink");
            let symlink_error =
                store.load_source(&PackagePath::new(root.to_path_buf()).expect("path"));
            assert!(matches!(
                symlink_error,
                Err(StoreError::SymlinkNotAllowed { .. })
            ));
        }
    }

    #[test]
    fn imports_stay_local_and_lock_writes_are_atomic() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("main");
        create_package_layout(&root);
        let imported = root.join("imports").join("core");
        create_package_layout(&imported);

        let store = FilesystemPackageStore::default();
        let import = ImportRef {
            package: PackageId::new("pkg.core").expect("package"),
            version: VersionRequirement::new("=1.0.0").expect("version requirement"),
            path: "imports/core".to_owned(),
            alias: None,
        };
        assert_eq!(import.package.as_str(), "pkg.core");
        assert_eq!(import.version.as_str(), "=1.0.0");

        let resolved = store
            .resolve_import(
                &PackagePath::new(root.clone()).expect("root path"),
                &import,
                None,
            )
            .expect("resolved import");
        assert_eq!(resolved.bundle().documents().len(), 3);

        for invalid in ["", "/tmp/pkg", "imports/../../escape"] {
            let invalid_import = ImportRef {
                package: PackageId::new("pkg.bad").expect("package"),
                version: VersionRequirement::new("=1.0.0").expect("version requirement"),
                path: invalid.to_owned(),
                alias: None,
            };
            let result = store.resolve_import(
                &PackagePath::new(root.clone()).expect("root path"),
                &invalid_import,
                None,
            );
            assert!(
                matches!(
                    result,
                    Err(StoreError::InvalidImportPath { .. })
                        | Err(StoreError::PathEscapesRoot { .. })
                        | Err(StoreError::Io { .. })
                ),
                "unexpected error for path `{invalid}`: {result:?}"
            );
        }

        let root_path = PackagePath::new(root.clone()).expect("root path");
        assert!(store.load_lock(&root_path).expect("load lock").is_none());

        write_file(root.join(LOCK_FILE_NAME), "{not-json");
        let corrupted = store.load_lock(&root_path);
        assert!(matches!(
            corrupted,
            Err(StoreError::InvalidLockEnvelope { .. })
        ));

        let lock = sample_lock();
        fs::remove_file(root.join(LOCK_FILE_NAME)).expect("remove corrupted lock");
        store.write_lock(&root_path, &lock).expect("write lock");
        let loaded = store.load_lock(&root_path).expect("load lock");
        assert!(loaded.is_some());

        fs::remove_file(root.join(LOCK_FILE_NAME)).expect("remove lock");
        let failing_store = FilesystemPackageStore::default().with_fail_before_rename(true);
        let write_result = failing_store.write_lock(&root_path, &lock);
        assert!(matches!(
            write_result,
            Err(StoreError::InjectedAtomicWriteFailure { .. })
        ));
        assert!(!root.join(LOCK_FILE_NAME).exists());
        assert!(!root.join(format!(".{LOCK_FILE_NAME}.tmp")).exists());
    }

    fn write_file(path: impl AsRef<Path>, content: &str) {
        fs::write(path, content).expect("write file");
    }

    fn create_package_layout(root: &Path) {
        fs::create_dir_all(root.join("rules")).expect("rules dir");
        fs::create_dir_all(root.join("scenarios")).expect("scenarios dir");
        write_file(root.join("rulery.yaml"), "name: main\n");
        write_file(root.join("vocabulary.yaml"), "roots: []\n");
        write_file(root.join("actions.yaml"), "actions: []\n");
    }

    fn sample_lock() -> RulebookLock {
        RulebookLock::new(
            LockedRoot::new(
                PackageId::new("pkg.main").expect("pkg.main"),
                Version::new("1.0.0").expect("version"),
                NormalizedSourceLocation::new("root").expect("location"),
                ContentHash::from_bytes([1; 32]),
            ),
            LanguageVersion::V1,
            vec![LockedImport::new(
                PackageId::new("pkg.core").expect("pkg.core"),
                Version::new("1.0.0").expect("version"),
                NormalizedSourceLocation::new("imports/core").expect("location"),
                ContentHash::from_bytes([2; 32]),
            )],
        )
        .expect("lock")
    }
}
