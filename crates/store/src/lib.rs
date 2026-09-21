//! Local package loading and integrity verification for Rulery.

#![forbid(unsafe_code)]

mod filesystem;
mod integrity;

pub use filesystem::{FilesystemPackageStore, ImportRef, PackageStore, StoreError};
