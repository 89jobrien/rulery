//! Exact temporal values and ports.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// Validated proleptic Gregorian policy date.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolicyDate {
    value: jiff::civil::Date,
    canonical: String,
}

impl PolicyDate {
    /// Parses an exact `YYYY-MM-DD` date in years 0001 through 9999.
    ///
    /// # Errors
    ///
    /// Returns [`TimeValueError`] for malformed or impossible dates.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TimeValueError> {
        let authored = value.as_ref();
        if authored.len() != 10 {
            return Err(TimeValueError::InvalidDate(authored.to_owned()));
        }
        let parsed = jiff::civil::Date::from_str(authored)
            .map_err(|_| TimeValueError::InvalidDate(authored.to_owned()))?;
        if parsed.year() < 1 || parsed.to_string() != authored {
            return Err(TimeValueError::InvalidDate(authored.to_owned()));
        }
        Ok(Self {
            value: parsed,
            canonical: authored.to_owned(),
        })
    }

    /// Returns the year component.
    #[must_use]
    pub fn year(&self) -> i16 {
        self.value.year()
    }
}

impl fmt::Display for PolicyDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical)
    }
}

/// UTC instant represented as signed Unix nanoseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UtcInstant {
    nanoseconds: i128,
    timestamp: jiff::Timestamp,
}

impl UtcInstant {
    /// Creates an instant when it can be represented by the supported civil calendar.
    ///
    /// # Errors
    ///
    /// Returns [`TimeValueError`] when the instant is outside Jiff's supported range.
    pub fn new(nanoseconds: i128) -> Result<Self, TimeValueError> {
        Ok(Self {
            nanoseconds,
            timestamp: timestamp(nanoseconds)?,
        })
    }

    /// Parses canonical signed Unix nanoseconds.
    ///
    /// # Errors
    ///
    /// Returns [`TimeValueError`] for non-canonical or out-of-range values.
    pub fn parse(value: &str) -> Result<Self, TimeValueError> {
        if !is_canonical_integer(value) {
            return Err(TimeValueError::InvalidInstant(value.to_owned()));
        }
        let nanoseconds = value
            .parse::<i128>()
            .map_err(|_| TimeValueError::InvalidInstant(value.to_owned()))?;
        Self::new(nanoseconds)
    }

    /// Formats this instant as UTC RFC 3339 with nine fractional digits.
    #[must_use]
    pub fn to_rfc3339(self) -> String {
        format!("{:.9}", self.timestamp)
    }

    /// Returns signed Unix nanoseconds.
    #[must_use]
    pub const fn as_nanoseconds(self) -> i128 {
        self.nanoseconds
    }
}

impl fmt::Display for UtcInstant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.nanoseconds.fmt(formatter)
    }
}

/// Exact signed duration in nanoseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DurationValue(i128);

impl DurationValue {
    /// Parses canonical signed nanoseconds.
    ///
    /// # Errors
    ///
    /// Returns [`TimeValueError`] for a non-canonical integer.
    pub fn parse(value: &str) -> Result<Self, TimeValueError> {
        if !is_canonical_integer(value) {
            return Err(TimeValueError::InvalidDuration(value.to_owned()));
        }
        value
            .parse::<i128>()
            .map(Self)
            .map_err(|_| TimeValueError::InvalidDuration(value.to_owned()))
    }
}

impl fmt::Display for DurationValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Validated IANA timezone name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolicyTimeZone(String);

impl PolicyTimeZone {
    /// Creates a syntactically valid timezone name.
    ///
    /// # Errors
    ///
    /// Returns [`TimeValueError`] for empty names or unsupported characters.
    pub fn new(value: impl Into<String>) -> Result<Self, TimeValueError> {
        let value = value.into();
        if !value.is_empty()
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+')
            })
        {
            Ok(Self(value))
        } else {
            Err(TimeValueError::InvalidTimeZone(value))
        }
    }

    /// Returns the timezone name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identity of the timezone database used for evaluation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TimeZoneDatabaseIdentity {
    implementation: String,
    version: String,
}

impl TimeZoneDatabaseIdentity {
    /// Creates a non-empty timezone database identity.
    ///
    /// # Errors
    ///
    /// Returns [`TimeValueError`] when either component is empty.
    pub fn new(
        implementation: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, TimeValueError> {
        let implementation = implementation.into();
        let version = version.into();
        if implementation.is_empty() || version.is_empty() {
            Err(TimeValueError::InvalidDatabaseIdentity)
        } else {
            Ok(Self {
                implementation,
                version,
            })
        }
    }
}

/// Date expiry boundary behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DateExpiryPolicy {
    /// The item remains valid through its expiry date.
    Inclusive,
    /// The item becomes invalid at the start of its expiry date.
    Exclusive,
}

/// Temporal semantics compiled into a decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeSemantics {
    timezone: PolicyTimeZone,
    expiry: DateExpiryPolicy,
}

impl TimeSemantics {
    /// Creates temporal semantics from validated components.
    #[must_use]
    pub fn new(timezone: PolicyTimeZone, expiry: DateExpiryPolicy) -> Self {
        Self { timezone, expiry }
    }
}

/// Supplies the current UTC instant to deterministic evaluation.
pub trait Clock: Send + Sync + 'static {
    /// Returns the current UTC instant.
    fn now_utc(&self) -> UtcInstant;
}

/// Converts UTC instants using a versioned timezone database.
pub trait TimeZoneDatabase: Send + Sync + 'static {
    /// Returns the implementation and database version.
    fn identity(&self) -> TimeZoneDatabaseIdentity;

    /// Converts an instant to a policy-local date.
    ///
    /// # Errors
    ///
    /// Returns [`TimeZoneError`] when the timezone cannot be resolved.
    fn local_date(
        &self,
        instant: UtcInstant,
        timezone: &PolicyTimeZone,
    ) -> Result<PolicyDate, TimeZoneError>;
}

/// Error returned for invalid temporal values.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TimeValueError {
    /// Invalid date text.
    #[error("invalid policy date `{0}`")]
    InvalidDate(String),
    /// Invalid instant text.
    #[error("invalid UTC instant `{0}`")]
    InvalidInstant(String),
    /// Invalid duration text.
    #[error("invalid duration `{0}`")]
    InvalidDuration(String),
    /// Invalid timezone name.
    #[error("invalid policy timezone `{0}`")]
    InvalidTimeZone(String),
    /// Empty timezone database component.
    #[error("timezone database identity components must be non-empty")]
    InvalidDatabaseIdentity,
}

/// Error returned by a timezone database adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("timezone `{timezone}` cannot resolve instant `{instant}`")]
pub struct TimeZoneError {
    /// Timezone name that failed.
    pub timezone: String,
    /// Instant that failed.
    pub instant: UtcInstant,
}

fn timestamp(nanoseconds: i128) -> Result<jiff::Timestamp, TimeValueError> {
    let seconds = nanoseconds.div_euclid(1_000_000_000);
    let fraction = nanoseconds.rem_euclid(1_000_000_000);
    let seconds = i64::try_from(seconds)
        .map_err(|_| TimeValueError::InvalidInstant(nanoseconds.to_string()))?;
    let fraction = i32::try_from(fraction)
        .map_err(|_| TimeValueError::InvalidInstant(nanoseconds.to_string()))?;
    jiff::Timestamp::new(seconds, fraction)
        .map_err(|_| TimeValueError::InvalidInstant(nanoseconds.to_string()))
}

fn is_canonical_integer(value: &str) -> bool {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    !unsigned.is_empty()
        && unsigned.bytes().all(|byte| byte.is_ascii_digit())
        && (unsigned == "0" || !unsigned.starts_with('0'))
        && value != "-0"
}

macro_rules! string_serde {
    ($type:ty, $parse:expr) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                $parse(&value).map_err(D::Error::custom)
            }
        }
    };
}

string_serde!(PolicyDate, PolicyDate::parse);
string_serde!(UtcInstant, UtcInstant::parse);
string_serde!(DurationValue, DurationValue::parse);
