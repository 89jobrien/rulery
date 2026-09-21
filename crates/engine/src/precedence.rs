//! Semantic precedence and conflict-preserving candidate selection.

use std::collections::BTreeSet;

use rulery_contracts::{Outcome, OutcomeKind, QualifiedRuleId};
use thiserror::Error;

/// Candidate precedence model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrecedenceModel {
    /// Outcome rank precedes priority.
    SafetyFirst,
    /// Priority precedes outcome rank.
    PriorityFirst,
    /// Explicit outcome rank precedes priority.
    ExplicitOutcome(ExplicitRanks),
    /// Priority precedes explicit outcome rank.
    ExplicitPriority(ExplicitRanks),
}

/// Validated explicit outcome ranks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplicitRanks {
    ranks: Vec<(OutcomeKind, u16)>,
}

impl ExplicitRanks {
    /// Creates a complete explicit rank map.
    ///
    /// # Errors
    ///
    /// Returns [`PrecedenceError`] unless all four kinds occur once with distinct ranks.
    pub fn new(ranks: Vec<(OutcomeKind, u16)>) -> Result<Self, PrecedenceError> {
        let kinds = ranks.iter().map(|(kind, _)| *kind).collect::<BTreeSet<_>>();
        if ranks.len() != 4 || kinds.len() != 4 {
            return Err(PrecedenceError::IncompleteExplicitRanks);
        }
        let values = ranks.iter().map(|(_, rank)| *rank).collect::<BTreeSet<_>>();
        if values.len() != 4 {
            return Err(PrecedenceError::DuplicateExplicitRank);
        }
        Ok(Self { ranks })
    }

    fn rank(&self, kind: OutcomeKind) -> Result<u16, PrecedenceError> {
        self.ranks
            .iter()
            .find_map(|(candidate, rank)| (*candidate == kind).then_some(*rank))
            .ok_or(PrecedenceError::IncompleteExplicitRanks)
    }

    fn validate(&self) -> Result<(), PrecedenceError> {
        Self::new(self.ranks.clone()).map(|_| ())
    }
}

/// Exact semantic precedence tuple.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(tag = "model", content = "components", rename_all = "snake_case")]
pub enum SemanticPrecedenceKey {
    /// `(outcome_rank, priority, specificity, override_rank)`.
    SafetyFirst(u16, i32, u32, u8),
    /// `(priority, outcome_rank, specificity, override_rank)`.
    PriorityFirst(i32, u16, u32, u8),
    /// Explicit outcome-primary tuple.
    ExplicitOutcome(u16, i32, u32, u8),
    /// Explicit priority-primary tuple.
    ExplicitPriority(i32, u16, u32, u8),
}

/// One true evaluation candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    /// Rule identity used only for presentation.
    pub rule: QualifiedRuleId,
    /// Instantiated outcome.
    pub outcome: Outcome,
    /// Signed authored priority.
    pub priority: i32,
    /// Static specificity.
    pub specificity: u32,
    /// Explicit override rank.
    pub override_rank: u8,
}

/// Candidate excluded by a lower semantic key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupersededCandidate {
    /// Superseded rule identity.
    pub rule: QualifiedRuleId,
    /// Candidate semantic key.
    pub key: SemanticPrecedenceKey,
}

/// Greatest-key incompatible candidates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictTrace {
    /// Greatest semantic key shared by all participants.
    pub key: SemanticPrecedenceKey,
    /// Participants sorted by rule identity for presentation only.
    pub participants: Vec<Candidate>,
}

/// Candidate selection result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Selection {
    /// Selected outcome, absent for conflict or empty input.
    pub outcome: Option<Outcome>,
    /// Determining rules sorted for presentation.
    pub determining_rules: Vec<QualifiedRuleId>,
    /// Lower-key candidates sorted for presentation.
    pub superseded: Vec<SupersededCandidate>,
    /// Greatest-key conflict details.
    pub conflict: Option<ConflictTrace>,
}

/// Explicit-rank or selection error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PrecedenceError {
    /// Explicit map did not contain all four outcome kinds exactly once.
    #[error("explicit precedence ranks must contain every outcome kind exactly once")]
    IncompleteExplicitRanks,
    /// Explicit ranks were not pairwise distinct.
    #[error("explicit precedence ranks must be pairwise distinct")]
    DuplicateExplicitRank,
}

/// Selects all candidates at the greatest semantic key without identity tie-breaking.
///
/// # Errors
///
/// Returns [`PrecedenceError`] when explicit rank configuration is invalid.
pub fn select_candidates(
    model: &PrecedenceModel,
    candidates: Vec<Candidate>,
) -> Result<Selection, PrecedenceError> {
    if let PrecedenceModel::ExplicitOutcome(ranks) | PrecedenceModel::ExplicitPriority(ranks) =
        model
    {
        ranks.validate()?;
    }
    if candidates.is_empty() {
        return Ok(empty_selection());
    }

    let keyed = candidates
        .into_iter()
        .map(|candidate| semantic_key(model, &candidate).map(|key| (key, candidate)))
        .collect::<Result<Vec<_>, PrecedenceError>>()?;
    let Some(greatest) = keyed.iter().map(|(key, _)| *key).max() else {
        return Ok(empty_selection());
    };
    let mut determining = keyed
        .iter()
        .filter(|(key, _)| *key == greatest)
        .map(|(_, candidate)| candidate.clone())
        .collect::<Vec<_>>();
    determining.sort_by(|left, right| left.rule.cmp(&right.rule));
    determining.dedup();

    let mut superseded = keyed
        .iter()
        .filter(|(key, _)| *key < greatest)
        .map(|(key, candidate)| SupersededCandidate {
            rule: candidate.rule.clone(),
            key: *key,
        })
        .collect::<Vec<_>>();
    superseded.sort_by(|left, right| left.rule.cmp(&right.rule));
    superseded.dedup();

    let Some(first_outcome) = determining
        .first()
        .map(|candidate| candidate.outcome.clone())
    else {
        return Ok(empty_selection());
    };
    if determining
        .iter()
        .all(|candidate| candidate.outcome == first_outcome)
    {
        let determining_rules = determining
            .into_iter()
            .map(|candidate| candidate.rule)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(Selection {
            outcome: Some(first_outcome),
            determining_rules,
            superseded,
            conflict: None,
        })
    } else {
        Ok(Selection {
            outcome: None,
            determining_rules: Vec::new(),
            superseded,
            conflict: Some(ConflictTrace {
                key: greatest,
                participants: determining,
            }),
        })
    }
}

fn empty_selection() -> Selection {
    Selection {
        outcome: None,
        determining_rules: Vec::new(),
        superseded: Vec::new(),
        conflict: None,
    }
}

pub(crate) fn semantic_key(
    model: &PrecedenceModel,
    candidate: &Candidate,
) -> Result<SemanticPrecedenceKey, PrecedenceError> {
    let outcome_rank = match model {
        PrecedenceModel::SafetyFirst | PrecedenceModel::PriorityFirst => {
            default_outcome_rank(candidate.outcome.kind())
        }
        PrecedenceModel::ExplicitOutcome(ranks) | PrecedenceModel::ExplicitPriority(ranks) => {
            ranks.rank(candidate.outcome.kind())?
        }
    };
    Ok(match model {
        PrecedenceModel::SafetyFirst => SemanticPrecedenceKey::SafetyFirst(
            outcome_rank,
            candidate.priority,
            candidate.specificity,
            candidate.override_rank,
        ),
        PrecedenceModel::PriorityFirst => SemanticPrecedenceKey::PriorityFirst(
            candidate.priority,
            outcome_rank,
            candidate.specificity,
            candidate.override_rank,
        ),
        PrecedenceModel::ExplicitOutcome(_) => SemanticPrecedenceKey::ExplicitOutcome(
            outcome_rank,
            candidate.priority,
            candidate.specificity,
            candidate.override_rank,
        ),
        PrecedenceModel::ExplicitPriority(_) => SemanticPrecedenceKey::ExplicitPriority(
            candidate.priority,
            outcome_rank,
            candidate.specificity,
            candidate.override_rank,
        ),
    })
}

const fn default_outcome_rank(kind: OutcomeKind) -> u16 {
    match kind {
        OutcomeKind::Deny => 4_000,
        OutcomeKind::Escalate => 3_000,
        OutcomeKind::RequestInformation => 2_000,
        OutcomeKind::Approve => 1_000,
    }
}

#[cfg(test)]
mod tests {
    use rulery_contracts::{EscalationId, PackageId, Reason, ReasonCode, Reasons, RuleId};

    use super::*;

    #[test]
    fn precedence_excludes_rule_identity_and_preserves_conflict() {
        assert_eq!(default_outcome_rank(OutcomeKind::Deny), 4_000);
        assert_eq!(default_outcome_rank(OutcomeKind::Escalate), 3_000);
        assert_eq!(default_outcome_rank(OutcomeKind::RequestInformation), 2_000);
        assert_eq!(default_outcome_rank(OutcomeKind::Approve), 1_000);
        assert_eq!(
            ExplicitRanks::new(vec![(OutcomeKind::Approve, 1)]),
            Err(PrecedenceError::IncompleteExplicitRanks)
        );
        assert_eq!(
            ExplicitRanks::new(vec![
                (OutcomeKind::Approve, 1),
                (OutcomeKind::Deny, 1),
                (OutcomeKind::Escalate, 2),
                (OutcomeKind::RequestInformation, 3),
            ]),
            Err(PrecedenceError::DuplicateExplicitRank)
        );
        let ranks = ExplicitRanks {
            ranks: vec![
                (OutcomeKind::Approve, 10),
                (OutcomeKind::Deny, 20),
                (OutcomeKind::Escalate, 30),
                (OutcomeKind::RequestInformation, 40),
            ],
        };
        let approve = candidate("rule.z", OutcomeKind::Approve, 100, 3, 0);
        let deny = candidate("rule.a", OutcomeKind::Deny, 1, 3, 0);

        let safety = select_candidates(
            &PrecedenceModel::SafetyFirst,
            vec![approve.clone(), deny.clone()],
        )
        .expect("safety");
        assert_eq!(
            safety.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Deny)
        );

        let priority = select_candidates(
            &PrecedenceModel::PriorityFirst,
            vec![approve.clone(), deny.clone()],
        )
        .expect("priority");
        assert_eq!(
            priority.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Approve)
        );

        let explicit_outcome = select_candidates(
            &PrecedenceModel::ExplicitOutcome(ranks.clone()),
            vec![approve.clone(), deny.clone()],
        )
        .expect("explicit outcome");
        assert_eq!(
            explicit_outcome.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Deny)
        );
        let explicit_priority = select_candidates(
            &PrecedenceModel::ExplicitPriority(ranks),
            vec![approve.clone(), deny.clone()],
        )
        .expect("explicit priority");
        assert_eq!(
            explicit_priority.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Approve)
        );

        let override_winner = candidate("rule.override", OutcomeKind::Approve, 100, 3, 1);
        let merged = select_candidates(
            &PrecedenceModel::PriorityFirst,
            vec![
                approve.clone(),
                override_winner.clone(),
                override_winner.clone(),
            ],
        )
        .expect("override");
        assert_eq!(merged.determining_rules, vec![override_winner.rule.clone()]);

        let tied_approve = candidate("rule.b", OutcomeKind::Approve, 7, 5, 0);
        let tied_approve_other = candidate("rule.a", OutcomeKind::Approve, 7, 5, 0);
        let forward = select_candidates(
            &PrecedenceModel::PriorityFirst,
            vec![tied_approve.clone(), tied_approve_other.clone()],
        )
        .expect("forward");
        let reverse = select_candidates(
            &PrecedenceModel::PriorityFirst,
            vec![tied_approve_other, tied_approve],
        )
        .expect("reverse");
        assert_eq!(forward, reverse);
        assert_eq!(forward.determining_rules[0].rule().as_str(), "rule.a");

        let conflict = select_candidates(
            &PrecedenceModel::PriorityFirst,
            vec![
                candidate("rule.escalate-a", OutcomeKind::Escalate, 9, 4, 0),
                candidate("rule.escalate-b", OutcomeKind::Escalate, 9, 4, 0),
            ],
        )
        .expect("conflict");
        assert!(conflict.outcome.is_none());
        assert!(conflict.determining_rules.is_empty());
        assert_eq!(
            conflict
                .conflict
                .expect("conflict trace")
                .participants
                .len(),
            2
        );
    }

    fn candidate(
        rule: &str,
        kind: OutcomeKind,
        priority: i32,
        specificity: u32,
        override_rank: u8,
    ) -> Candidate {
        let reasons = Reasons::new(vec![
            Reason::new(ReasonCode::new("test").expect("reason code"), "test").expect("reason"),
        ])
        .expect("reasons");
        let outcome = match kind {
            OutcomeKind::Approve => Outcome::approve(reasons, Vec::new()),
            OutcomeKind::Deny => Outcome::deny(reasons, Vec::new()),
            OutcomeKind::Escalate => Outcome::escalate(
                EscalationId::new(format!("destination.{rule}")).expect("destination"),
                reasons,
                Vec::new(),
            ),
            OutcomeKind::RequestInformation => unreachable!("not needed by fixture"),
        };
        Candidate {
            rule: QualifiedRuleId::new(
                PackageId::new("pkg.main").expect("package"),
                RuleId::new(rule).expect("rule"),
            ),
            outcome,
            priority,
            specificity,
            override_rank,
        }
    }
}
