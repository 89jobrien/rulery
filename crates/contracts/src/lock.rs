//! Versioned local package lock contracts.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ContentHash, LanguageVersion, NormalizedSourceLocation, PackageId, Version};

/// Locked root package identity and byte integrity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedRoot {
    package: PackageId,
    version: Version,
    source: NormalizedSourceLocation,
    content_hash: ContentHash,
}

impl LockedRoot {
    /// Creates a locked root from validated boundary values.
    #[must_use]
    pub fn new(
        package: PackageId,
        version: Version,
        source: NormalizedSourceLocation,
        content_hash: ContentHash,
    ) -> Self {
        Self {
            package,
            version,
            source,
            content_hash,
        }
    }
}

/// One locked transitive import.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedImport {
    package: PackageId,
    version: Version,
    source: NormalizedSourceLocation,
    content_hash: ContentHash,
}

impl LockedImport {
    /// Creates a locked import from validated boundary values.
    #[must_use]
    pub fn new(
        package: PackageId,
        version: Version,
        source: NormalizedSourceLocation,
        content_hash: ContentHash,
    ) -> Self {
        Self {
            package,
            version,
            source,
            content_hash,
        }
    }
}

/// Rulebook lock payload schema version 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulebookLockV1 {
    root: LockedRoot,
    language_version: LanguageVersion,
    imports: Vec<LockedImport>,
}

/// Validated rulebook lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RulebookLock {
    payload: RulebookLockV1,
}

impl RulebookLock {
    /// Creates a lock with imports sorted by package identity.
    ///
    /// # Errors
    ///
    /// Returns [`LockError`] when an imported package identity appears more than once.
    pub fn new(
        root: LockedRoot,
        language_version: LanguageVersion,
        mut imports: Vec<LockedImport>,
    ) -> Result<Self, LockError> {
        imports.sort_by(|left, right| left.package.cmp(&right.package));
        let unique: BTreeSet<_> = imports.iter().map(|import| &import.package).collect();
        if unique.len() != imports.len() {
            return Err(LockError::DuplicateImport);
        }
        Ok(Self {
            payload: RulebookLockV1 {
                root,
                language_version,
                imports,
            },
        })
    }

    /// Returns the validated version 1 payload.
    #[must_use]
    pub fn payload(&self) -> &RulebookLockV1 {
        &self.payload
    }
}

/// Versioned rulebook lock envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum RulebookLockEnvelope {
    /// Rulebook lock schema version 1.
    #[serde(rename = "rulery.rulebook-lock/v1")]
    V1(RulebookLockV1),
}

impl From<RulebookLock> for RulebookLockEnvelope {
    fn from(lock: RulebookLock) -> Self {
        Self::V1(lock.payload)
    }
}

/// Error returned when a rulebook lock violates its invariants.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LockError {
    /// Two imports use the same package identity.
    #[error("a rulebook lock cannot contain duplicate package imports")]
    DuplicateImport,
}
