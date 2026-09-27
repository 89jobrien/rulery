//! Rule reachability findings.

use rulery_contracts::{ContentHash, QualifiedRuleId};
use serde::{Deserialize, Serialize};

use crate::WitnessCase;

/// Reachability classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachabilityStatus {
    /// A witness reaches the rule.
    Reachable,
    /// A proof establishes no satisfying assignment.
    Unreachable,
    /// Available domains or budget cannot decide.
    Inconclusive,
}

/// Sound unreachability evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofCertificate {
    /// Proof method identity.
    pub method: String,
    /// Hash of proved constraints.
    pub constraints_hash: ContentHash,
}

/// Reachability result for one rule.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleReachability {
    /// Rule identity.
    pub rule: QualifiedRuleId,
    /// Reachability status.
    pub status: ReachabilityStatus,
    /// Minimal replayable witness for reachable rules.
    pub witness: Option<WitnessCase>,
    /// Proof for unreachable rules.
    pub proof: Option<ProofCertificate>,
}
