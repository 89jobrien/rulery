//! Validated source and package boundary values.

use std::{fmt, path::Path, path::PathBuf, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

use crate::{FactSegment, FactSegmentError};

/// Error returned when a source or package boundary value is invalid.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid {kind} value `{value}`")]
pub struct BoundaryError {
    kind: &'static str,
    value: String,
}

impl BoundaryError {
    fn new(kind: &'static str, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }
}

/// Dot-separated path to a fact value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FactPath {
    segments: Vec<FactSegment>,
}

impl FactPath {
    /// Returns the number of path segments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Returns whether the path has no segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns the validated path segments.
    #[must_use]
    pub fn segments(&self) -> &[FactSegment] {
        &self.segments
    }
}

impl FromStr for FactPath {
    type Err = BoundaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(BoundaryError::new("fact path", value));
        }
        let segments = value
            .split('.')
            .map(FactSegment::from_str)
            .collect::<Result<Vec<_>, FactSegmentError>>()
            .map_err(|_| BoundaryError::new("fact path", value))?;
        Ok(Self { segments })
    }
}

impl fmt::Display for FactPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut segments = self.segments.iter();
        if let Some(first) = segments.next() {
            first.fmt(formatter)?;
        }
        for segment in segments {
            write!(formatter, ".{segment}")?;
        }
        Ok(())
    }
}

impl Serialize for FactPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FactPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(D::Error::custom)
    }
}

/// Fact path retained before vocabulary resolution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct UnresolvedFactPath(FactPath);

impl FromStr for UnresolvedFactPath {
    type Err = BoundaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        FactPath::from_str(value).map(Self)
    }
}

impl fmt::Display for UnresolvedFactPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for UnresolvedFactPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(D::Error::custom)
    }
}

macro_rules! normalized_path_type {
    ($(#[$metadata:meta])* $name:ident, $kind:literal) => {
        $(#[$metadata])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates a normalized relative path.
            ///
            /// # Errors
            ///
            /// Returns [`BoundaryError`] for empty, absolute, escaping, or non-canonical paths.
            pub fn new(value: impl Into<String>) -> Result<Self, BoundaryError> {
                let value = value.into();
                if is_normalized_relative_path(&value) {
                    Ok(Self(value))
                } else {
                    Err(BoundaryError::new($kind, value))
                }
            }

            /// Returns the normalized path text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(D::Error::custom)
            }
        }
    };
}

normalized_path_type!(/// Path to a source file inside a package.
    SourcePath, "source path");
normalized_path_type!(/// Canonical local location of an imported package.
    NormalizedSourceLocation, "source location");

fn is_normalized_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Filesystem path to a package root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackagePath(PathBuf);

impl PackagePath {
    /// Creates a non-empty package path.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when the supplied path is empty.
    pub fn new(value: impl Into<PathBuf>) -> Result<Self, BoundaryError> {
        let value = value.into();
        if value.as_os_str().is_empty() {
            Err(BoundaryError::new("package path", ""))
        } else {
            Ok(Self(value))
        }
    }
}

impl AsRef<Path> for PackagePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Canonical semantic version.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Version {
    parsed: semver::Version,
    canonical: String,
}

impl Version {
    /// Parses a canonical semantic version.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when parsing fails or the input is not canonical.
    pub fn new(value: impl Into<String>) -> Result<Self, BoundaryError> {
        let value = value.into();
        let parsed = semver::Version::parse(&value)
            .map_err(|_| BoundaryError::new("version", value.clone()))?;
        if parsed.to_string() != value {
            return Err(BoundaryError::new("version", value));
        }
        Ok(Self {
            parsed,
            canonical: value,
        })
    }

    /// Returns the canonical semantic version text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

/// Canonical semantic-version requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionRequirement {
    parsed: semver::VersionReq,
    canonical: String,
}

impl VersionRequirement {
    /// Parses a semantic-version requirement.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when parsing fails.
    pub fn new(value: impl Into<String>) -> Result<Self, BoundaryError> {
        let value = value.into();
        let parsed = semver::VersionReq::parse(&value)
            .map_err(|_| BoundaryError::new("version requirement", value.clone()))?;
        Ok(Self {
            parsed,
            canonical: value,
        })
    }

    /// Returns the authored version requirement.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Returns whether a version satisfies this requirement.
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        self.parsed.matches(&version.parsed)
    }
}

/// Supported Rulery language version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct LanguageVersion(u16);

impl LanguageVersion {
    /// Current language version.
    pub const V1: Self = Self(1);

    /// Creates a supported language version.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] for values other than version 1.
    pub fn new(value: u16) -> Result<Self, BoundaryError> {
        if value == 1 {
            Ok(Self(value))
        } else {
            Err(BoundaryError::new("language version", value.to_string()))
        }
    }

    /// Returns the numeric language version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for LanguageVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}
