//! Validated source and package boundary values.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fmt,
    path::Path,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

use crate::{FactSegment, FactSegmentError, SourceId};

/// Compact identity of a source file inside a source map.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceKey(u32);

impl SourceKey {
    /// Creates a source key.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the numeric source key.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for SourceKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceKeyVisitor;

        impl serde::de::Visitor<'_> for SourceKeyVisitor {
            type Value = SourceKey;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a u32 source key or its decimal string")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u32::try_from(value).map(SourceKey::new).map_err(E::custom)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                value.parse::<u32>().map(SourceKey::new).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(SourceKeyVisitor)
    }
}

impl Serialize for SourceKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// Half-open UTF-8 byte span in a source file.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    source: SourceKey,
    start: u32,
    end: u32,
}

impl Serialize for Span {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct WireSpan {
            source: String,
            start: String,
            end: String,
        }
        WireSpan {
            source: self.source.get().to_string(),
            start: self.start.to_string(),
            end: self.end.to_string(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Span {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireSpan {
            source: String,
            start: String,
            end: String,
        }
        let wire = WireSpan::deserialize(deserializer)?;
        let source = wire.source.parse::<u32>().map_err(D::Error::custom)?;
        let start = wire.start.parse::<u32>().map_err(D::Error::custom)?;
        let end = wire.end.parse::<u32>().map_err(D::Error::custom)?;
        if start > end {
            return Err(D::Error::custom("span start must not exceed end"));
        }
        Ok(Self {
            source: SourceKey::new(source),
            start,
            end,
        })
    }
}

impl Span {
    /// Returns the source key.
    #[must_use]
    pub const fn source(self) -> SourceKey {
        self.source
    }

    /// Returns the inclusive start byte.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Returns the exclusive end byte.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }
}

/// Source file content and stable identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    id: SourceId,
    path: SourcePath,
    content: Arc<str>,
}

impl SourceFile {
    /// Creates a source file from validated identity and path values.
    #[must_use]
    pub fn new(id: SourceId, path: SourcePath, content: Arc<str>) -> Self {
        Self { id, path, content }
    }

    /// Returns the source path.
    #[must_use]
    pub fn path(&self) -> &SourcePath {
        &self.path
    }

    /// Returns the stable source identity.
    #[must_use]
    pub fn id(&self) -> &SourceId {
        &self.id
    }

    /// Returns the source text.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Collection of source files keyed for compact spans.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMap {
    entries: BTreeMap<SourceKey, SourceFile>,
}

impl SourceMap {
    /// Creates an empty source map.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Inserts a source file under a unique key.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when the key already exists.
    pub fn insert(&mut self, key: SourceKey, file: SourceFile) -> Result<(), BoundaryError> {
        match self.entries.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(file);
                Ok(())
            }
            Entry::Occupied(_) => Err(BoundaryError::new("source key", key.get().to_string())),
        }
    }

    /// Returns a source file by key.
    #[must_use]
    pub fn get(&self, key: SourceKey) -> Option<&SourceFile> {
        self.entries.get(&key)
    }

    /// Iterates source files in source-key order.
    pub fn iter(&self) -> impl Iterator<Item = (SourceKey, &SourceFile)> {
        self.entries.iter().map(|(key, file)| (*key, file))
    }

    /// Returns the number of source files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether this source map has no files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Creates a validated half-open span.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] for unknown keys, reversed bounds, out-of-range offsets, or
    /// offsets that split a UTF-8 code point.
    pub fn span(&self, source: SourceKey, start: u32, end: u32) -> Result<Span, BoundaryError> {
        let file = self
            .entries
            .get(&source)
            .ok_or_else(|| BoundaryError::new("source key", source.get().to_string()))?;
        let start_index = usize::try_from(start)
            .map_err(|_| BoundaryError::new("span start", start.to_string()))?;
        let end_index =
            usize::try_from(end).map_err(|_| BoundaryError::new("span end", end.to_string()))?;
        if start > end
            || end_index > file.content.len()
            || !file.content.is_char_boundary(start_index)
            || !file.content.is_char_boundary(end_index)
        {
            return Err(BoundaryError::new("source span", format!("{start}..{end}")));
        }
        Ok(Span { source, start, end })
    }

    /// Resolves a byte offset to a one-based line and Unicode-scalar column.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] for an unknown source or invalid UTF-8 boundary.
    pub fn line_column(&self, source: SourceKey, offset: u32) -> Result<(u32, u32), BoundaryError> {
        let file = self
            .entries
            .get(&source)
            .ok_or_else(|| BoundaryError::new("source key", source.get().to_string()))?;
        let offset = usize::try_from(offset)
            .map_err(|_| BoundaryError::new("source offset", offset.to_string()))?;
        if offset > file.content.len() || !file.content.is_char_boundary(offset) {
            return Err(BoundaryError::new("source offset", offset.to_string()));
        }
        let prefix = &file.content[..offset];
        let line = u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1)
            .map_err(|_| BoundaryError::new("line", offset.to_string()))?;
        let column = u32::try_from(
            prefix
                .rsplit('\n')
                .next()
                .map_or(0, |line| line.chars().count())
                + 1,
        )
        .map_err(|_| BoundaryError::new("column", offset.to_string()))?;
        Ok((line, column))
    }
}

/// Raw source document before source-key assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDocument {
    path: SourcePath,
    content: Arc<str>,
}

impl SourceDocument {
    /// Creates a raw source document.
    #[must_use]
    pub fn new(path: SourcePath, content: Arc<str>) -> Self {
        Self { path, content }
    }

    /// Returns the normalized source path.
    #[must_use]
    pub fn path(&self) -> &SourcePath {
        &self.path
    }

    /// Returns the raw source text.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Deterministically ordered package source documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceBundle {
    documents: Vec<SourceDocument>,
}

impl SourceBundle {
    /// Creates a bundle sorted by normalized source path.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when two documents have the same path.
    pub fn new(mut documents: Vec<SourceDocument>) -> Result<Self, BoundaryError> {
        documents.sort_by(|left, right| left.path.cmp(&right.path));
        if documents
            .windows(2)
            .any(|pair| pair[0].path == pair[1].path)
        {
            return Err(BoundaryError::new("source bundle", "duplicate path"));
        }
        Ok(Self { documents })
    }

    /// Returns source documents in deterministic path order.
    #[must_use]
    pub fn documents(&self) -> &[SourceDocument] {
        &self.documents
    }
}

/// Integrity bytes associated with a loaded source bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceIntegrity {
    digest: [u8; 32],
}

impl SourceIntegrity {
    /// Creates source integrity from a raw digest.
    #[must_use]
    pub const fn new(digest: [u8; 32]) -> Self {
        Self { digest }
    }

    /// Returns the raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Loaded source bundle and its integrity metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSourceBundle {
    bundle: SourceBundle,
    integrity: SourceIntegrity,
}

impl LoadedSourceBundle {
    /// Creates a loaded bundle with its verified integrity.
    #[must_use]
    pub fn new(bundle: SourceBundle, integrity: SourceIntegrity) -> Self {
        Self { bundle, integrity }
    }

    /// Returns the source bundle.
    #[must_use]
    pub fn bundle(&self) -> &SourceBundle {
        &self.bundle
    }

    /// Returns the source integrity.
    #[must_use]
    pub const fn integrity(&self) -> &SourceIntegrity {
        &self.integrity
    }
}

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

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
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

impl fmt::Display for VersionRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for VersionRequirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VersionRequirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
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
