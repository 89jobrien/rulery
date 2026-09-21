//! Strict scenario result wire contracts.

use rulery_contracts::{ContentHash, ScenarioId};
use rulery_engine::DecisionTraceV1;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Scenario execution status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStatus {
    /// Every expectation matched.
    Passed,
    /// Evaluation succeeded but expectations differed.
    Failed,
    /// Scenario evidence was invalid.
    Invalid,
    /// Evaluation failed before producing a trace.
    Error,
}

/// Compared expectation field.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioExpectationField {
    /// Outcome kind.
    Outcome,
    /// Determining rule set.
    DeterminingRules,
    /// Required fact set.
    RequiredFacts,
    /// Reason code set.
    ReasonCodes,
}

/// One exact scenario mismatch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFailure {
    /// Mismatched field.
    pub field: ScenarioExpectationField,
    /// Exact expected JSON.
    pub expected: serde_json::Value,
    /// Exact actual JSON.
    pub actual: serde_json::Value,
}

/// Scenario result payload version 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResultV1 {
    /// Compiled package hash.
    pub package_hash: ContentHash,
    /// Scenario identity.
    pub scenario: ScenarioId,
    /// Result status.
    pub status: ScenarioStatus,
    /// Ordered exact mismatches.
    pub failures: Vec<ScenarioFailure>,
    /// Decision trace when evaluation produced one.
    pub trace: Option<DecisionTraceV1>,
}

/// Validated scenario result wrapper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioResult {
    payload: ScenarioResultV1,
}

impl ScenarioResult {
    /// Validates a scenario result payload.
    ///
    /// # Errors
    ///
    /// Returns [`ScenarioResultError`] when status and failure collection disagree.
    pub fn new(payload: ScenarioResultV1) -> Result<Self, ScenarioResultError> {
        let valid = match payload.status {
            ScenarioStatus::Failed => !payload.failures.is_empty(),
            ScenarioStatus::Passed | ScenarioStatus::Invalid | ScenarioStatus::Error => {
                payload.failures.is_empty()
            }
        };
        if valid {
            Ok(Self { payload })
        } else {
            Err(ScenarioResultError::InvalidStatusFailures)
        }
    }

    /// Returns the validated v1 payload.
    #[must_use]
    pub const fn payload(&self) -> &ScenarioResultV1 {
        &self.payload
    }

    pub(crate) fn from_validated(payload: ScenarioResultV1) -> Self {
        debug_assert!(Self::new(payload.clone()).is_ok());
        Self { payload }
    }
}

/// Strict versioned scenario result envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum ScenarioResultEnvelope {
    /// Scenario result schema v1.
    #[serde(rename = "rulery.scenario-result/v1")]
    V1(ScenarioResultV1),
}

/// Scenario result invariant error.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ScenarioResultError {
    /// Status and failures disagree.
    #[error("scenario status and failures are inconsistent")]
    InvalidStatusFailures,
}
