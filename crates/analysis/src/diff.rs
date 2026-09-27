//! Semantic policy diff classification.

use std::collections::BTreeSet;

use rulery_contracts::{DecisionId, Outcome, OutcomeKind};
use serde::{Deserialize, Serialize};

use crate::{AnalysisCompleteness, WitnessCase};

/// Normative structural policy change class.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralChange {
    /// Expected or default outcome changed.
    Default,
    /// Precedence configuration changed.
    Precedence,
    /// Missing-fact strategy changed.
    MissingStrategy,
    /// Invalid-fact strategy changed.
    InvalidStrategy,
    /// Timezone changed.
    Timezone,
    /// Expiry policy changed.
    Expiry,
    /// Resolved vocabulary changed.
    Vocabulary,
    /// Import integrity changed.
    ImportIntegrity,
    /// Reason codes or messages changed.
    Reasons,
    /// Required fact set changed.
    RequiredFacts,
    /// Declarative actions changed.
    Actions,
}

/// One paired evaluation over a union partition cell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffCell {
    /// Decision identity.
    pub decision: DecisionId,
    /// Before outcome.
    pub before: Outcome,
    /// After outcome.
    pub after: Outcome,
    /// Structural changes explaining equal-kind behavior changes.
    pub structural: BTreeSet<StructuralChange>,
    /// Paired replay witness using the same instant and timezone database.
    pub witness: WitnessCase,
}

/// Semantic outcome change classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeChangeKind {
    /// After policy permits a previously restricted case.
    MorePermissive,
    /// After policy restricts a previously permitted case.
    MoreRestrictive,
    /// After policy introduces escalation.
    IntroducesEscalation,
    /// After policy introduces an information request.
    IntroducesInformationRequest,
    /// Only semantic precedence changed selection.
    PrecedenceOnly,
    /// Only reasons changed.
    ReasonOnly,
    /// Change does not fit a narrower category.
    Unknown,
}

/// One semantic outcome change with evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeChange {
    /// Before outcome.
    pub before: Outcome,
    /// After outcome.
    pub after: Outcome,
    /// Exact classification.
    pub classification: OutcomeChangeKind,
    /// Replayable paired witness.
    pub witness: WitnessCase,
    /// Stable diagnostic code.
    pub diagnostic_code: String,
    /// Structural change evidence.
    pub structural: BTreeSet<StructuralChange>,
}

/// Semantic diff for one decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDiff {
    /// Decision identity.
    pub decision: DecisionId,
    /// Reported changes.
    pub changes: Vec<OutcomeChange>,
    /// Diff completeness.
    pub completeness: AnalysisCompleteness,
    /// `Some(true)` only when complete enumeration proved no changes.
    pub unchanged: Option<bool>,
}

/// Semantic policy differ.
#[derive(Clone, Debug, Default)]
pub struct PolicyDiffer;

impl PolicyDiffer {
    /// Classifies paired union-partition evaluations under a state limit.
    ///
    /// Cells are examined in caller-provided order, so callers must supply canonical union-
    /// partition order for deterministic truncation. When `max_states` excludes any cell,
    /// completeness is inconclusive and [`PolicyDiff::unchanged`] is `None`; no equivalence claim
    /// is made from the examined prefix.
    #[must_use]
    pub fn diff(&self, decision: DecisionId, cells: Vec<DiffCell>, max_states: u64) -> PolicyDiff {
        let available = usize::try_from(max_states).unwrap_or(usize::MAX);
        let complete = cells.len() <= available;
        let examined = cells.len().min(available) as u64;
        let changes = cells
            .into_iter()
            .take(available)
            .filter(|cell| cell.before != cell.after || !cell.structural.is_empty())
            .map(|cell| {
                let classification = classify(&cell);
                let diagnostic_code = diagnostic_code(classification, &cell.structural);
                OutcomeChange {
                    before: cell.before,
                    after: cell.after,
                    classification,
                    witness: cell.witness,
                    diagnostic_code: diagnostic_code.to_owned(),
                    structural: cell.structural,
                }
            })
            .collect::<Vec<_>>();
        let unchanged = complete.then_some(changes.is_empty());
        PolicyDiff {
            decision,
            changes,
            completeness: if complete {
                AnalysisCompleteness::Complete
            } else {
                AnalysisCompleteness::Inconclusive {
                    examined,
                    limit: max_states,
                }
            },
            unchanged,
        }
    }
}

fn classify(cell: &DiffCell) -> OutcomeChangeKind {
    if cell.structural == BTreeSet::from([StructuralChange::Precedence]) {
        return OutcomeChangeKind::PrecedenceOnly;
    }
    if cell.structural == BTreeSet::from([StructuralChange::Reasons]) {
        return OutcomeChangeKind::ReasonOnly;
    }
    match (cell.before.kind(), cell.after.kind()) {
        (_, OutcomeKind::Escalate) if cell.before.kind() != OutcomeKind::Escalate => {
            OutcomeChangeKind::IntroducesEscalation
        }
        (_, OutcomeKind::RequestInformation)
            if cell.before.kind() != OutcomeKind::RequestInformation =>
        {
            OutcomeChangeKind::IntroducesInformationRequest
        }
        (OutcomeKind::Deny | OutcomeKind::Escalate, OutcomeKind::Approve) => {
            OutcomeChangeKind::MorePermissive
        }
        (OutcomeKind::Approve, OutcomeKind::Deny) => OutcomeChangeKind::MoreRestrictive,
        _ => OutcomeChangeKind::Unknown,
    }
}

fn diagnostic_code(
    classification: OutcomeChangeKind,
    structural: &BTreeSet<StructuralChange>,
) -> &'static str {
    match classification {
        OutcomeChangeKind::MoreRestrictive => "RUL351",
        OutcomeChangeKind::MorePermissive => "RUL352",
        OutcomeChangeKind::PrecedenceOnly => "RUL354",
        OutcomeChangeKind::Unknown if structural.contains(&StructuralChange::Default) => "RUL353",
        OutcomeChangeKind::IntroducesEscalation
        | OutcomeChangeKind::IntroducesInformationRequest
        | OutcomeChangeKind::ReasonOnly
        | OutcomeChangeKind::Unknown => "RUL350",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rulery_contracts::{
        EscalationId, Reason, ReasonCode, Reasons, RequiredFacts, TimeZoneDatabaseIdentity,
        UtcInstant,
    };

    use crate::{WitnessClaim, minimize_witness};

    use super::*;

    #[test]
    fn semantic_diff_reports_every_normative_change_class() {
        let decision = DecisionId::new("decision.main").expect("decision");
        let cases = vec![
            cell(deny(), approve(), BTreeSet::new()),
            cell(approve(), deny(), BTreeSet::new()),
            cell(approve(), escalate(), BTreeSet::new()),
            cell(approve(), request(), BTreeSet::new()),
            cell(
                approve(),
                approve_other(),
                BTreeSet::from([StructuralChange::Precedence]),
            ),
            cell(
                approve(),
                approve_other(),
                BTreeSet::from([StructuralChange::Reasons]),
            ),
            cell(
                approve(),
                approve_other(),
                BTreeSet::from([
                    StructuralChange::Default,
                    StructuralChange::MissingStrategy,
                    StructuralChange::InvalidStrategy,
                    StructuralChange::Timezone,
                    StructuralChange::Expiry,
                    StructuralChange::Vocabulary,
                    StructuralChange::ImportIntegrity,
                    StructuralChange::RequiredFacts,
                    StructuralChange::Actions,
                ]),
            ),
        ];
        let report = PolicyDiffer.diff(decision.clone(), cases, 100);
        assert_eq!(
            report
                .changes
                .iter()
                .map(|change| change.classification)
                .collect::<Vec<_>>(),
            vec![
                OutcomeChangeKind::MorePermissive,
                OutcomeChangeKind::MoreRestrictive,
                OutcomeChangeKind::IntroducesEscalation,
                OutcomeChangeKind::IntroducesInformationRequest,
                OutcomeChangeKind::PrecedenceOnly,
                OutcomeChangeKind::ReasonOnly,
                OutcomeChangeKind::Unknown,
            ]
        );
        assert_eq!(
            report
                .changes
                .iter()
                .map(|change| change.diagnostic_code.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["RUL350", "RUL351", "RUL352", "RUL353", "RUL354"])
        );
        assert!(report.changes.iter().all(|change| change.witness.at
            == UtcInstant::new(1).expect("instant")
            && change.witness.timezone_database
                == TimeZoneDatabaseIdentity::new("test", "1").expect("database")));
        assert_eq!(report.completeness, AnalysisCompleteness::Complete);
        assert_eq!(report.unchanged, Some(false));

        let exhausted =
            PolicyDiffer.diff(decision, vec![cell(approve(), deny(), BTreeSet::new())], 0);
        assert!(matches!(
            exhausted.completeness,
            AnalysisCompleteness::Inconclusive {
                examined: 0,
                limit: 0
            }
        ));
        assert_eq!(exhausted.unchanged, None);
        assert!(exhausted.changes.is_empty());
    }

    fn cell(before: Outcome, after: Outcome, structural: BTreeSet<StructuralChange>) -> DiffCell {
        DiffCell {
            decision: DecisionId::new("decision.main").expect("decision"),
            before,
            after,
            structural,
            witness: witness(),
        }
    }
    fn witness() -> WitnessCase {
        minimize_witness(
            "diff",
            BTreeMap::new(),
            UtcInstant::new(1).expect("instant"),
            TimeZoneDatabaseIdentity::new("test", "1").expect("database"),
            BTreeSet::new(),
            WitnessClaim::BehaviorChange,
            |_, _, _| true,
        )
    }
    fn reasons(code: &str) -> Reasons {
        Reasons::new(vec![
            Reason::new(ReasonCode::new(code).expect("code"), code).expect("reason"),
        ])
        .expect("reasons")
    }
    fn approve() -> Outcome {
        Outcome::approve(reasons("approve"), Vec::new())
    }
    fn approve_other() -> Outcome {
        Outcome::approve(reasons("other"), Vec::new())
    }
    fn deny() -> Outcome {
        Outcome::deny(reasons("deny"), Vec::new())
    }
    fn escalate() -> Outcome {
        Outcome::escalate(
            EscalationId::new("review").expect("destination"),
            reasons("escalate"),
            Vec::new(),
        )
    }
    fn request() -> Outcome {
        Outcome::request_information(
            RequiredFacts::new(BTreeSet::from(["member.value".parse().expect("path")]))
                .expect("facts"),
            reasons("request"),
            Vec::new(),
        )
    }
}
