//! Missing and invalid evidence strategy application.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{EscalationId, FactPath};

use crate::Truth;

/// Compiled missing-fact behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MissingFactStrategy {
    /// Retain unknown truth.
    PreserveUnknown,
    /// Convert absent predicate results to false before composition.
    ClosedWorldFalse,
    /// Produce an information-request candidate.
    RequestInformation,
    /// Produce an escalation candidate.
    Escalate {
        /// Escalation destination.
        destination: EscalationId,
    },
}

/// Compiled malformed-fact behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvalidFactStrategy {
    /// Stop evaluation when malformed relevant evidence exists.
    RejectEvaluation,
    /// Retain invalid truth.
    PreserveInvalid,
    /// Produce an escalation candidate.
    Escalate {
        /// Escalation destination.
        destination: EscalationId,
    },
}

/// Evidence class attached to a decision-relevant predicate result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceClass {
    /// Valid or explicit-null evidence.
    Supplied,
    /// Absent evidence.
    Absent,
    /// Malformed evidence.
    Malformed,
}

/// One decision-relevant predicate observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredicateObservation {
    /// Referenced fact path.
    pub path: FactPath,
    /// Predicate truth before strategy application.
    pub truth: Truth,
    /// Evidence class.
    pub evidence: EvidenceClass,
    /// Rule priority.
    pub priority: i64,
    /// Rule specificity.
    pub specificity: u32,
    /// Static override rank.
    pub override_rank: u8,
}

/// Generated strategy reason.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StrategyReason {
    /// Exact reason code.
    pub code: &'static str,
    /// Exact generated message.
    pub message: &'static str,
}

/// Generated strategy outcome.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StrategyOutcome {
    /// Request missing facts.
    RequestInformation,
    /// Escalate to a declared destination.
    Escalate(EscalationId),
}

/// Generated candidate entering ordinary precedence selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrategyCandidate {
    /// Generated outcome.
    pub outcome: StrategyOutcome,
    /// Aggregated rule priority.
    pub priority: i64,
    /// Aggregated specificity at greatest priority.
    pub specificity: u32,
    /// Aggregated override rank at preceding key components.
    pub override_rank: u8,
    /// Sorted missing facts for information requests.
    pub required_facts: BTreeSet<FactPath>,
    /// Canonical generated reasons.
    pub reasons: BTreeSet<StrategyReason>,
}

/// Strategy application output retained for tracing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrategyApplication {
    /// Predicate truths after pre-composition strategy changes.
    pub adjusted_truths: Vec<Truth>,
    /// Canonically sorted generated candidates.
    pub candidates: Vec<StrategyCandidate>,
    /// Human-readable strategy trace records.
    pub trace: Vec<String>,
}

/// Evaluation failure caused by strategy policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluationError {
    /// Decision-relevant malformed fact rejected evaluation.
    InvalidFact {
        /// Sorted decision-relevant malformed paths.
        paths: BTreeSet<FactPath>,
    },
}

/// Applies compiled missing/invalid strategies to relevant predicate observations.
///
/// # Errors
///
/// Returns [`EvaluationError::InvalidFact`] when `RejectEvaluation` sees relevant malformed facts.
#[allow(clippy::too_many_lines)]
pub fn apply_strategies(
    observations: &[PredicateObservation],
    missing: &MissingFactStrategy,
    invalid: &InvalidFactStrategy,
) -> Result<StrategyApplication, EvaluationError> {
    let missing_paths = observations
        .iter()
        .filter(|observation| observation.evidence == EvidenceClass::Absent)
        .map(|observation| observation.path.clone())
        .collect::<BTreeSet<_>>();
    let invalid_paths = observations
        .iter()
        .filter(|observation| observation.evidence == EvidenceClass::Malformed)
        .map(|observation| observation.path.clone())
        .collect::<BTreeSet<_>>();

    if !invalid_paths.is_empty() && *invalid == InvalidFactStrategy::RejectEvaluation {
        return Err(EvaluationError::InvalidFact {
            paths: invalid_paths,
        });
    }

    let adjusted_truths = observations
        .iter()
        .map(|observation| {
            if *missing == MissingFactStrategy::ClosedWorldFalse
                && observation.evidence == EvidenceClass::Absent
            {
                Truth::False
            } else {
                observation.truth
            }
        })
        .collect();
    let mut candidates = BTreeMap::new();
    let mut trace = Vec::new();

    if !missing_paths.is_empty() {
        match missing {
            MissingFactStrategy::PreserveUnknown => {
                trace.push("missing:preserve-unknown".to_owned());
            }
            MissingFactStrategy::ClosedWorldFalse => {
                trace.push("missing:closed-world-false".to_owned());
            }
            MissingFactStrategy::RequestInformation => {
                insert_candidate(
                    &mut candidates,
                    candidate(
                        StrategyOutcome::RequestInformation,
                        observations,
                        EvidenceClass::Absent,
                        &missing_paths,
                        StrategyReason {
                            code: "missing-required-facts",
                            message: "Decision-relevant facts are missing.",
                        },
                    ),
                );
                trace.push("missing:request-information".to_owned());
            }
            MissingFactStrategy::Escalate { destination } => {
                insert_candidate(
                    &mut candidates,
                    candidate(
                        StrategyOutcome::Escalate(destination.clone()),
                        observations,
                        EvidenceClass::Absent,
                        &BTreeSet::new(),
                        StrategyReason {
                            code: "missing-facts-escalated",
                            message: "Decision-relevant facts are missing.",
                        },
                    ),
                );
                trace.push(format!("missing:escalate:{destination}"));
            }
        }
    }

    if !invalid_paths.is_empty() {
        match invalid {
            InvalidFactStrategy::RejectEvaluation => {
                unreachable!("handled before candidate generation")
            }
            InvalidFactStrategy::PreserveInvalid => {
                trace.push("invalid:preserve-invalid".to_owned());
            }
            InvalidFactStrategy::Escalate { destination } => {
                insert_candidate(
                    &mut candidates,
                    candidate(
                        StrategyOutcome::Escalate(destination.clone()),
                        observations,
                        EvidenceClass::Malformed,
                        &BTreeSet::new(),
                        StrategyReason {
                            code: "invalid-facts-escalated",
                            message: "Decision-relevant facts are invalid.",
                        },
                    ),
                );
                trace.push(format!("invalid:escalate:{destination}"));
            }
        }
    }

    Ok(StrategyApplication {
        adjusted_truths,
        candidates: candidates.into_values().collect(),
        trace,
    })
}

type CandidateKey = (StrategyOutcome, i64, u32, u8);

fn candidate(
    outcome: StrategyOutcome,
    observations: &[PredicateObservation],
    evidence: EvidenceClass,
    required_facts: &BTreeSet<FactPath>,
    reason: StrategyReason,
) -> StrategyCandidate {
    // Compute a realizable lexicographic key: specificity is maximized only inside the greatest
    // priority subset, then override rank only inside the greatest priority/specificity subset.
    let priority = observations
        .iter()
        .filter(|entry| entry.evidence == evidence)
        .map(|entry| entry.priority)
        .max()
        .unwrap_or(0);
    let specificity = observations
        .iter()
        .filter(|entry| entry.evidence == evidence && entry.priority == priority)
        .map(|entry| entry.specificity)
        .max()
        .unwrap_or(0);
    let override_rank = observations
        .iter()
        .filter(|entry| {
            entry.evidence == evidence
                && entry.priority == priority
                && entry.specificity == specificity
        })
        .map(|entry| entry.override_rank)
        .max()
        .unwrap_or(0);
    StrategyCandidate {
        outcome,
        priority,
        specificity,
        override_rank,
        required_facts: required_facts.clone(),
        reasons: BTreeSet::from([reason]),
    }
}

fn insert_candidate(
    candidates: &mut BTreeMap<CandidateKey, StrategyCandidate>,
    candidate: StrategyCandidate,
) {
    let key = (
        candidate.outcome.clone(),
        candidate.priority,
        candidate.specificity,
        candidate.override_rank,
    );
    if let Some(existing) = candidates.get_mut(&key) {
        existing.required_facts.extend(candidate.required_facts);
        existing.reasons.extend(candidate.reasons);
    } else {
        candidates.insert(key, candidate);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn strategies_create_exact_candidates_and_reasons() {
        let observations = vec![
            observation(
                "member.email",
                Truth::Unknown,
                EvidenceClass::Absent,
                5,
                2,
                0,
            ),
            observation(
                "member.phone",
                Truth::Unknown,
                EvidenceClass::Absent,
                5,
                4,
                1,
            ),
            observation(
                "member.age",
                Truth::Invalid,
                EvidenceClass::Malformed,
                5,
                4,
                1,
            ),
            observation("member.name", Truth::True, EvidenceClass::Supplied, 8, 1, 0),
        ];

        let closed = apply_strategies(
            &observations,
            &MissingFactStrategy::ClosedWorldFalse,
            &InvalidFactStrategy::PreserveInvalid,
        )
        .expect("closed world");
        assert_eq!(
            closed.adjusted_truths,
            vec![Truth::False, Truth::False, Truth::Invalid, Truth::True]
        );

        let requested = apply_strategies(
            &observations,
            &MissingFactStrategy::RequestInformation,
            &InvalidFactStrategy::PreserveInvalid,
        )
        .expect("request");
        let request = &requested.candidates[0];
        assert_eq!(request.outcome, StrategyOutcome::RequestInformation);
        assert_eq!(request.priority, 5);
        assert_eq!(request.specificity, 4);
        assert_eq!(
            request
                .required_facts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["member.email", "member.phone"]
        );
        assert!(request.reasons.contains(&StrategyReason {
            code: "missing-required-facts",
            message: "Decision-relevant facts are missing.",
        }));

        let destination = EscalationId::new("review.safety").expect("destination");
        let escalated = apply_strategies(
            &observations,
            &MissingFactStrategy::Escalate {
                destination: destination.clone(),
            },
            &InvalidFactStrategy::Escalate {
                destination: destination.clone(),
            },
        )
        .expect("escalate");
        assert_eq!(escalated.candidates.len(), 1);
        assert_eq!(
            escalated.candidates[0].outcome,
            StrategyOutcome::Escalate(destination)
        );
        assert_eq!(escalated.candidates[0].reasons.len(), 2);
        assert!(escalated.candidates[0].reasons.contains(&StrategyReason {
            code: "missing-facts-escalated",
            message: "Decision-relevant facts are missing.",
        }));
        assert!(escalated.candidates[0].reasons.contains(&StrategyReason {
            code: "invalid-facts-escalated",
            message: "Decision-relevant facts are invalid.",
        }));

        assert!(matches!(
            apply_strategies(
                &observations,
                &MissingFactStrategy::PreserveUnknown,
                &InvalidFactStrategy::RejectEvaluation,
            ),
            Err(EvaluationError::InvalidFact { paths }) if paths.iter().map(ToString::to_string).collect::<Vec<_>>() == vec!["member.age"]
        ));

        let preserved = apply_strategies(
            &observations,
            &MissingFactStrategy::PreserveUnknown,
            &InvalidFactStrategy::PreserveInvalid,
        )
        .expect("preserved");
        assert_eq!(
            preserved.adjusted_truths,
            observations
                .iter()
                .map(|entry| entry.truth)
                .collect::<Vec<_>>()
        );
    }

    fn observation(
        path: &str,
        truth: Truth,
        evidence: EvidenceClass,
        priority: i64,
        specificity: u32,
        override_rank: u8,
    ) -> PredicateObservation {
        PredicateObservation {
            path: FactPath::from_str(path).expect("path"),
            truth,
            evidence,
            priority,
            specificity,
            override_rank,
        }
    }
}
