//! Stable diagnostic identity and classification.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use thiserror::Error;

macro_rules! diagnostic_code_constants {
    ($($name:ident = $value:literal;)+) => {
        $(
            #[doc = concat!("Stable diagnostic code `", $value, "`.")]
            pub const $name: &'static str = $value;
        )+
    };
}

/// Stable public diagnostic identifier with the form `RUL` and three digits.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    diagnostic_code_constants! {
        SYNTAX_INVALID = "RUL001";
        DUPLICATE_DECLARATION = "RUL002";
        UNKNOWN_SYMBOL = "RUL003";
        INVALID_FACT_PATH = "RUL004";
        TYPE_MISMATCH = "RUL005";
        SOURCE_RESOURCE_LIMIT = "RUL006";
        UNDEFINED_OPERATIONAL_TERM = "RUL100";
        TEMPORAL_TERM_UNDEFINED = "RUL101";
        MISSING_OUTCOME_REASON = "RUL102";
        UNKNOWN_FACT_POLICY_UNHANDLED = "RUL150";
        DEFAULT_OUTCOME_MISSING = "RUL151";
        PRECEDENCE_AMBIGUOUS = "RUL152";
        TIMEZONE_UNAVAILABLE = "RUL153";
        CONFLICTING_RULES = "RUL200";
        SHADOWED_APPROVAL = "RUL201";
        UNREACHABLE_RULE = "RUL202";
        REDUNDANT_RULE = "RUL203";
        UNDECLARED_OVERRIDE = "RUL204";
        RUNTIME_CONFLICT = "RUL205";
        UNCOVERED_COMBINATION = "RUL250";
        UNHANDLED_ENUM_VARIANT = "RUL251";
        UNHANDLED_MISSING_FACT = "RUL252";
        TEMPORAL_BOUNDARY_UNCOVERED = "RUL253";
        ANALYSIS_INCONCLUSIVE = "RUL254";
        SCENARIO_FAILED = "RUL300";
        SCENARIO_UNDECLARED_FACT = "RUL301";
        STALE_SCENARIO = "RUL302";
        OUTCOME_CHANGED = "RUL350";
        MORE_RESTRICTIVE_CHANGE = "RUL351";
        MORE_PERMISSIVE_CHANGE = "RUL352";
        DEFAULT_CHANGED = "RUL353";
        PRECEDENCE_CHANGED = "RUL354";
        LOSSY_TARGET_EXPORT = "RUL400";
        UNSUPPORTED_TARGET_OUTCOME = "RUL401";
        LOCKFILE_OUT_OF_DATE = "RUL500";
        IMPORT_HASH_CHANGED = "RUL501";
        LOCKFILE_MISSING = "RUL502";
        IMPORT_CYCLE = "RUL503";
        IMPORT_RESOURCE_LIMIT = "RUL504";
        INTERNAL_INVARIANT = "RUL900";
    }

    /// Creates a well-formed diagnostic code.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticCodeError`] unless the value matches `RUL[0-9]{3}` exactly.
    pub fn new(value: impl Into<String>) -> Result<Self, DiagnosticCodeError> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.len() == 6
            && bytes.starts_with(b"RUL")
            && bytes[3..].iter().all(u8::is_ascii_digit)
        {
            Ok(Self(value))
        } else {
            Err(DiagnosticCodeError::InvalidFormat { value })
        }
    }

    /// Returns the canonical code text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn known(value: &'static str) -> Self {
        Self(value.to_owned())
    }

    /// Returns the registered family or the reserved/internal fallback.
    #[must_use]
    pub fn family(&self) -> DiagnosticFamily {
        crate::diagnostic_definition(self).map_or_else(
            || {
                if self.0.as_bytes()[3] == b'9' {
                    DiagnosticFamily::Internal
                } else {
                    DiagnosticFamily::Reserved
                }
            },
            crate::DiagnosticDefinition::family_value,
        )
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for DiagnosticCode {
    type Err = DiagnosticCodeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

/// Invalid diagnostic code error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DiagnosticCodeError {
    /// Code text did not match the required grammar.
    #[error("diagnostic code `{value}` must have the form RUL followed by three digits")]
    InvalidFormat {
        /// Rejected code text.
        value: String,
    },
}

/// Diagnostic family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticFamily {
    /// Source syntax diagnostics.
    Syntax,
    /// Vocabulary diagnostics.
    Vocabulary,
    /// Policy clarity diagnostics.
    Clarity,
    /// Decision semantic diagnostics.
    Semantics,
    /// Interacting rule diagnostics.
    RuleInteraction,
    /// Decision coverage diagnostics.
    Coverage,
    /// Scenario diagnostics.
    Scenario,
    /// Semantic change diagnostics.
    Change,
    /// Target export diagnostics.
    Export,
    /// Integrity and provenance diagnostics.
    Evidence,
    /// Unassigned diagnostic range.
    Reserved,
    /// Internal invariant diagnostics.
    Internal,
}
/// Intrinsic diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Severity {
    /// Evaluation or compilation cannot safely continue.
    Error,
    /// Actionable concern that does not necessarily block output.
    Warning,
    /// Informational improvement guidance.
    Advice,
}
/// Required structured evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceRequirement {
    /// Labels and properties are sufficient.
    None,
    /// Package or compiler provenance is required.
    Provenance,
    /// A replayable witness is required.
    Witness,
    /// An evaluation trace is required.
    Trace,
    /// Before-and-after behavioral evidence is required.
    BehaviorChange,
    /// Analysis budget evidence is required.
    AnalysisLimit,
    /// A static proof is required.
    Proof,
    /// Runtime conflict evidence is required.
    Conflict,
    /// Structured properties are required.
    Properties,
    /// One of several evidence forms is required.
    OneOf(Vec<Self>),
}
/// Subsystem producing a diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticProducer {
    /// Package storage adapter.
    Store,
    /// Source parser.
    Parser,
    /// Policy compiler.
    Compiler,
    /// Decision evaluator.
    Evaluator,
    /// Static analyzer.
    Analyzer,
    /// Scenario compiler.
    ScenarioCompiler,
    /// Scenario runner.
    ScenarioRunner,
    /// Artifact renderer.
    Renderer,
    /// Public facade composition layer.
    Facade,
}
/// Whether and how a diagnostic can be suppressed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Suppressibility {
    /// Suppression is prohibited.
    Never,
    /// Suppression requires an authored justification.
    WithJustification,
    /// Suppression requires justification and expiry.
    WithJustificationAndExpiry,
}
