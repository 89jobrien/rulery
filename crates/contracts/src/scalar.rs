//! Exact decimal scalar values.

use std::{fmt, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// Exact canonical decimal value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DecimalValue {
    value: Decimal,
    canonical: String,
}

impl DecimalValue {
    /// Parses and canonicalizes a non-exponent decimal string.
    ///
    /// # Errors
    ///
    /// Returns [`DecimalValueError`] when the input is not a supported decimal spelling.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DecimalValueError> {
        let authored = value.as_ref();
        if !is_decimal(authored) {
            return Err(DecimalValueError::InvalidFormat {
                value: authored.to_owned(),
            });
        }
        let parsed =
            Decimal::from_str_exact(authored).map_err(|_| DecimalValueError::InvalidFormat {
                value: authored.to_owned(),
            })?;
        let value = if parsed.is_zero() {
            Decimal::ZERO
        } else {
            parsed.normalize()
        };
        Ok(Self {
            canonical: value.to_string(),
            value,
        })
    }

    /// Returns the canonical decimal string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl fmt::Display for DecimalValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for DecimalValue {
    type Err = DecimalValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for DecimalValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DecimalValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

/// Error returned for an invalid decimal value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DecimalValueError {
    /// The supplied text is not an accepted decimal spelling.
    #[error("decimal `{value}` has invalid format")]
    InvalidFormat {
        /// Rejected decimal text.
        value: String,
    },
}

fn is_decimal(value: &str) -> bool {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    if unsigned.is_empty() || value.starts_with('+') || value.contains(['e', 'E']) {
        return false;
    }
    let mut parts = unsigned.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
    {
        return false;
    }
    fraction
        .is_none_or(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
}
