//! Versioned renderer-owned wire envelopes.

use serde::{Deserialize, Serialize};

use crate::DecisionTableV1;

/// Strict decision-table envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum DecisionTableEnvelope {
    /// Decision-table schema version 1.
    #[serde(rename = "rulery.decision-table/v1")]
    V1(DecisionTableV1),
}
