//! Canonical content hashing contracts.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// BLAKE3-256 content hash.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentHash {
    bytes: [u8; 32],
}

impl ContentHash {
    /// Creates a content hash from raw digest bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Parses the exact `blake3:` wire representation.
    ///
    /// # Errors
    ///
    /// Returns [`ContentHashError`] for unknown algorithms, uppercase hexadecimal, or a digest
    /// whose decoded length is not 32 bytes.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ContentHashError> {
        let authored = value.as_ref();
        let encoded = authored
            .strip_prefix("blake3:")
            .ok_or_else(|| ContentHashError(authored.to_owned()))?;
        if encoded.len() != 64
            || !encoded
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ContentHashError(authored.to_owned()));
        }
        let decoded = hex::decode(encoded).map_err(|_| ContentHashError(authored.to_owned()))?;
        let bytes =
            <[u8; 32]>::try_from(decoded).map_err(|_| ContentHashError(authored.to_owned()))?;
        Ok(Self { bytes })
    }

    /// Hashes exact unframed bytes.
    #[must_use]
    pub fn digest(bytes: &[u8]) -> Self {
        Self::from_bytes(*blake3::hash(bytes).as_bytes())
    }

    /// Returns the raw BLAKE3 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Returns lowercase hexadecimal without the algorithm prefix.
    #[must_use]
    pub fn to_hex(self) -> String {
        hex::encode(self.bytes)
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "blake3:{}", hex::encode(self.bytes))
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

/// Error returned when a content hash violates its wire format.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid content hash `{0}`")]
pub struct ContentHashError(String);

/// Domain separating each canonical semantic hash.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HashDomain {
    /// Compiled package payload version 1.
    CompiledPackageV1,
    /// Case facts payload version 1.
    CaseFactsV1,
    /// Decision trace payload version 1.
    DecisionTraceV1,
    /// Evaluation identity version 1.
    EvaluationV1,
}

impl HashDomain {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::CompiledPackageV1 => b"rulery.compiled-package.v1",
            Self::CaseFactsV1 => b"rulery.case-facts.v1",
            Self::DecisionTraceV1 => b"rulery.decision-trace.v1",
            Self::EvaluationV1 => b"rulery.evaluation.v1",
        }
    }
}

/// Hashes ordered, length-prefixed byte parts under a versioned domain.
///
/// The frame is `u64::to_be_bytes(length) || bytes` for the domain and then for each part. Bytes
/// are consumed exactly as supplied: no text, path, line-ending, or Unicode normalization occurs.
/// Domain labels and framing are compatibility-sensitive wire contracts.
#[must_use]
pub fn hash_parts<'a>(
    domain: HashDomain,
    parts: impl IntoIterator<Item = &'a [u8]>,
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    let domain = domain.as_bytes();
    hasher.update(&(domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    ContentHash::from_bytes(*hasher.finalize().as_bytes())
}

/// Deterministic evaluation identity.
pub type EvaluationId = ContentHash;
