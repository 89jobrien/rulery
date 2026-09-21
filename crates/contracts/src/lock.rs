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

    /// Returns the root package identity.
    #[must_use]
    pub fn package(&self) -> &PackageId {
        &self.package
    }

    /// Returns the root package version.
    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Returns the normalized root source location.
    #[must_use]
    pub fn source(&self) -> &NormalizedSourceLocation {
        &self.source
    }

    /// Returns the root source content hash.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
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

    /// Returns the imported package identity.
    #[must_use]
    pub fn package(&self) -> &PackageId {
        &self.package
    }

    /// Returns the resolved package version.
    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Returns the normalized source location.
    #[must_use]
    pub fn source(&self) -> &NormalizedSourceLocation {
        &self.source
    }

    /// Returns the imported source content hash.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
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

impl RulebookLockV1 {
    /// Returns locked root metadata.
    #[must_use]
    pub const fn root(&self) -> &LockedRoot {
        &self.root
    }

    /// Returns the locked language version.
    #[must_use]
    pub const fn language_version(&self) -> LanguageVersion {
        self.language_version
    }

    /// Returns sorted transitive imports.
    #[must_use]
    pub fn imports(&self) -> &[LockedImport] {
        &self.imports
    }
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

    /// Reconstructs a validated lock from an owned v1 payload.
    #[must_use]
    pub fn from_payload(payload: RulebookLockV1) -> Self {
        Self { payload }
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

impl From<RulebookLockEnvelope> for RulebookLock {
    fn from(value: RulebookLockEnvelope) -> Self {
        match value {
            RulebookLockEnvelope::V1(payload) => Self::from_payload(payload),
        }
    }
}

/// Error returned when a rulebook lock violates its invariants.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LockError {
    /// Two imports use the same package identity.
    #[error("a rulebook lock cannot contain duplicate package imports")]
    DuplicateImport,
}
