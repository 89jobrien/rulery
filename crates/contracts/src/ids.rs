//! Validated stable identifiers.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// Maximum encoded length of a stable identifier.
const MAX_ID_LEN: usize = 128;

/// Validated stable identifier used by Rulery boundary types.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct StableId {
    value: String,
}

impl StableId {
    /// Creates an identifier after validating its canonical wire grammar.
    ///
    /// # Errors
    ///
    /// Returns [`StableIdError`] when the value is empty, too long, non-ASCII, or contains an
    /// invalid character or boundary.
    pub fn new(value: impl Into<String>) -> Result<Self, StableIdError> {
        let value = value.into();
        if is_stable_id(&value) {
            Ok(Self { value })
        } else {
            Err(StableIdError::InvalidFormat { value })
        }
    }

    /// Returns the canonical identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for StableId {
    type Err = StableIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl AsRef<str> for StableId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'de> Deserialize<'de> for StableId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

/// Error returned when a stable identifier violates its wire grammar.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StableIdError {
    /// The supplied text is not a canonical stable identifier.
    #[error("stable identifier `{value}` has invalid format")]
    InvalidFormat {
        /// Rejected identifier text.
        value: String,
    },
}

fn is_stable_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=MAX_ID_LEN).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

macro_rules! define_typed_id {
    ($(#[$metadata:meta])* $name:ident) => {
        $(#[$metadata])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(StableId);

        impl $name {
            /// Creates a typed identifier after validating its stable identifier grammar.
            ///
            /// # Errors
            ///
            /// Returns [`StableIdError`] when the supplied value is invalid.
            pub fn new(value: impl Into<String>) -> Result<Self, StableIdError> {
                StableId::new(value).map(Self)
            }

            /// Returns the canonical identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = StableIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
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

define_typed_id!(/// Package identity.
    PackageId);
define_typed_id!(/// Decision identity.
    DecisionId);
define_typed_id!(/// Rule identity.
    RuleId);
define_typed_id!(/// Vocabulary type identity.
    TypeId);
define_typed_id!(/// Predicate identity.
    PredicateId);
define_typed_id!(/// Action identity.
    ActionId);
define_typed_id!(/// Scenario identity.
    ScenarioId);
define_typed_id!(/// Escalation destination identity.
    EscalationId);
define_typed_id!(/// Source document identity.
    SourceId);
define_typed_id!(/// Root fact identity.
    FactRootId);
define_typed_id!(/// Outcome reason identity.
    ReasonCode);

/// One validated segment of a fact path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FactSegment {
    value: String,
}

impl FactSegment {
    /// Creates a segment that cannot contain the fact-path separator.
    ///
    /// # Errors
    ///
    /// Returns [`FactSegmentError`] when the segment violates its canonical grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, FactSegmentError> {
        let value = value.into();
        if is_stable_id(&value) && !value.contains('.') {
            Ok(Self { value })
        } else {
            Err(FactSegmentError::InvalidFormat { value })
        }
    }

    /// Returns the canonical segment text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for FactSegment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for FactSegment {
    type Err = FactSegmentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for FactSegment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

/// Error returned when a fact segment violates its wire grammar.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum FactSegmentError {
    /// The supplied text is not a canonical fact segment.
    #[error("fact segment `{value}` has invalid format")]
    InvalidFormat {
        /// Rejected segment text.
        value: String,
    },
}

/// Rule identity qualified by its owning package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QualifiedRuleId {
    package: PackageId,
    rule: RuleId,
}

impl QualifiedRuleId {
    /// Creates a qualified rule identity from validated components.
    #[must_use]
    pub fn new(package: PackageId, rule: RuleId) -> Self {
        Self { package, rule }
    }

    /// Returns the package component.
    #[must_use]
    pub fn package(&self) -> &PackageId {
        &self.package
    }

    /// Returns the rule component.
    #[must_use]
    pub fn rule(&self) -> &RuleId {
        &self.rule
    }
}

impl fmt::Display for QualifiedRuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}::{}", self.package, self.rule)
    }
}

impl Serialize for QualifiedRuleId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl FromStr for QualifiedRuleId {
    type Err = QualifiedRuleIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (package, rule) = value
            .split_once("::")
            .filter(|(_, rule)| !rule.contains("::"))
            .ok_or_else(|| QualifiedRuleIdError::InvalidFormat {
                value: value.to_owned(),
            })?;
        Ok(Self::new(
            PackageId::new(package).map_err(QualifiedRuleIdError::Package)?,
            RuleId::new(rule).map_err(QualifiedRuleIdError::Rule)?,
        ))
    }
}

impl<'de> Deserialize<'de> for QualifiedRuleId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(D::Error::custom)
    }
}

/// Error returned when parsing a qualified rule identity.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum QualifiedRuleIdError {
    /// The `package::rule` separator is absent or repeated.
    #[error("qualified rule `{value}` must have the form package::rule")]
    InvalidFormat {
        /// Rejected qualified identity text.
        value: String,
    },
    /// The package component is invalid.
    #[error("invalid package component: {0}")]
    Package(StableIdError),
    /// The rule component is invalid.
    #[error("invalid rule component: {0}")]
    Rule(StableIdError),
}
