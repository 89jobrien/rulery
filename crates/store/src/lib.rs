//! Local-only package loading, integrity verification, import resolution, and lock persistence.
//!
//! [`FilesystemPackageStore`] selects the exact authored layout, preserves source bytes, rejects
//! path escapes and symlinked source files, and computes deterministic bundle framing. Lock writes
//! use a same-directory temporary file and atomic rename; this crate performs no network access.

#![forbid(unsafe_code)]

mod filesystem;
mod integrity;

pub use filesystem::{FilesystemPackageStore, ImportRef, PackageStore, StoreError};
