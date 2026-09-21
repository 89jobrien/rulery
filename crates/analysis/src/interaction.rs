//! Reachability and pairwise rule interaction analysis.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{ContentHash, QualifiedRuleId};
use rulery_engine::{Candidate, PrecedenceModel};
use serde::{Deserialize, Serialize};

use crate::{ProofCertificate, ReachabilityStatus, RuleReachability, WitnessCase};

/// Pairwise overlap classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapClassification {
    /// Outcomes are compatible under precedence.
    Compatible,
    /// Equal-key outcomes conflict.
    Conflict,
    /// A restrictive winner shadows approval.
    ShadowedApproval,
    /// Conditions and outcomes are redundant.
    Redundant,
    /// Pair contains an explicit override relation.
    ExplicitOverride,
}

/// Pairwise overlap finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleOverlap {
    /// Left rule identity.
    pub left: QualifiedRuleId,
    /// Right rule identity.
    pub right: QualifiedRuleId,
    /// Semantic classification.
    pub classification: OverlapClassification,
    /// Minimal replayable overlap witness.
    pub witness: WitnessCase,
}

/// Input semantics for one rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleAnalysisInput {
    /// Candidate instantiated when the rule matches.
    pub candidate: Candidate,
    /// `Some(true/false)` when satisfiability is known.
    pub satisfiable: Option<bool>,
    /// Canonical condition identity used for redundancy.
    pub condition_hash: ContentHash,
    /// Rules this condition can overlap.
    pub overlaps: BTreeSet<QualifiedRuleId>,
    /// Whether this is a declared explicit override.
    pub explicit_override: bool,
    /// Whether an override relationship lacks a declaration.
    pub undeclared_override: bool,
    /// Replayable witness for satisfiable states.
    pub witness: WitnessCase,
}

/// Diagnostic-like analysis finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisFinding {
    /// Stable RUL code.
    pub code: &'static str,
    /// Rules supporting the finding.
    pub rules: Vec<QualifiedRuleId>,
}

/// Combined reachability and interaction analysis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionAnalysis {
    /// Per-rule reachability.
    pub reachability: Vec<RuleReachability>,
    /// Pairwise overlaps.
    pub overlaps: Vec<RuleOverlap>,
    /// Stable findings.
    pub findings: Vec<AnalysisFinding>,
}

/// Runs deterministic reachability and pairwise interaction passes.
#[must_use]
pub fn analyze_interactions(
    model: &PrecedenceModel,
    mut rules: Vec<RuleAnalysisInput>,
) -> InteractionAnalysis {
    rules.sort_by(|left, right| left.candidate.rule.cmp(&right.candidate.rule));
    let by_id = rules
        .iter()
        .enumerate()
        .map(|(index, rule)| (rule.candidate.rule.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut findings = Vec::new();
    let reachability = rules
        .iter()
        .map(|rule| match rule.satisfiable {
            Some(true) => RuleReachability {
                rule: rule.candidate.rule.clone(),
                status: ReachabilityStatus::Reachable,
                witness: Some(rule.witness.clone()),
                proof: None,
            },
            Some(false) => {
                findings.push(finding("RUL202", vec![rule.candidate.rule.clone()]));
                RuleReachability {
                    rule: rule.candidate.rule.clone(),
                    status: ReachabilityStatus::Unreachable,
                    witness: None,
                    proof: Some(ProofCertificate {
                        method: "finite-partition-exhaustion".to_owned(),
                        constraints_hash: rule.condition_hash,
                    }),
                }
            }
            None => RuleReachability {
                rule: rule.candidate.rule.clone(),
                status: ReachabilityStatus::Inconclusive,
                witness: None,
                proof: None,
            },
        })
        .collect();

    let mut overlaps = Vec::new();
    for left in &rules {
        for right_id in &left.overlaps {
            let Some(right) = by_id.get(right_id).map(|index| &rules[*index]) else {
                continue;
            };
            let classification = classify_pair(model, left, right);
            if let Some(code) = finding_code(classification) {
                findings.push(finding(
                    code,
                    vec![left.candidate.rule.clone(), right.candidate.rule.clone()],
                ));
            }
            let mut witness = left.witness.clone();
            witness.claim = crate::WitnessClaim::RuleOverlap(
                left.candidate.rule.clone(),
                right.candidate.rule.clone(),
            );
            overlaps.push(RuleOverlap {
                left: left.candidate.rule.clone(),
                right: right.candidate.rule.clone(),
                classification,
                witness,
            });
        }
    }
    overlaps.sort_by(|left, right| {
        left.left
            .cmp(&right.left)
            .then_with(|| left.right.cmp(&right.right))
    });
    findings.sort_by(|left, right| {
        left.code
            .cmp(right.code)
            .then_with(|| left.rules.cmp(&right.rules))
    });
    InteractionAnalysis {
        reachability,
        overlaps,
        findings,
    }
}

fn classify_pair(
    model: &PrecedenceModel,
    left: &RuleAnalysisInput,
    right: &RuleAnalysisInput,
) -> OverlapClassification {
    // Classification order is semantic: override evidence outranks redundancy, which outranks
    // precedence conflict/shadowing. Callers must provide each overlap in one canonical direction
    // to avoid duplicate pair findings.
    if left.explicit_override
        || right.explicit_override
        || left.undeclared_override
        || right.undeclared_override
    {
        return OverlapClassification::ExplicitOverride;
    }
    if left.condition_hash == right.condition_hash
        && left.candidate.outcome == right.candidate.outcome
    {
        return OverlapClassification::Redundant;
    }
    let selection = rulery_engine::select_candidates(
        model,
        vec![left.candidate.clone(), right.candidate.clone()],
    );
    let Ok(selection) = selection else {
        return OverlapClassification::Compatible;
    };
    if selection.conflict.is_some() {
        return OverlapClassification::Conflict;
    }
    if selection
        .outcome
        .as_ref()
        .is_some_and(|outcome| outcome.kind() != rulery_contracts::OutcomeKind::Approve)
        && (left.candidate.outcome.kind() == rulery_contracts::OutcomeKind::Approve
            || right.candidate.outcome.kind() == rulery_contracts::OutcomeKind::Approve)
    {
        OverlapClassification::ShadowedApproval
    } else {
        OverlapClassification::Compatible
    }
}

const fn finding_code(classification: OverlapClassification) -> Option<&'static str> {
    match classification {
        OverlapClassification::Compatible => None,
        OverlapClassification::Conflict => Some("RUL200"),
        OverlapClassification::ShadowedApproval => Some("RUL201"),
        OverlapClassification::Redundant => Some("RUL203"),
        OverlapClassification::ExplicitOverride => Some("RUL204"),
    }
}

fn finding(code: &'static str, mut rules: Vec<QualifiedRuleId>) -> AnalysisFinding {
    rules.sort();
    AnalysisFinding { code, rules }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rulery_contracts::{
        FactPath, Outcome, PackageId, Reason, ReasonCode, Reasons, RuleId,
        TimeZoneDatabaseIdentity, UtcInstant, Value,
    };

    use crate::{WitnessClaim, minimize_witness};

    use super::*;

    #[test]
    fn interaction_findings_are_sound_and_evidenced() {
        let ids = [
            "reachable",
            "unreachable",
            "compatible",
            "conflict",
            "shadowed",
            "redundant",
            "override",
        ];
        let mut rules = ids.iter().map(|id| rule(id)).collect::<Vec<_>>();
        rules[1].satisfiable = Some(false);
        rules[5].condition_hash = rules[0].condition_hash;
        rules[5].candidate.outcome = rules[0].candidate.outcome.clone();
        rules[6].explicit_override = true;
        rules[6].undeclared_override = true;
        for (left, right) in [(0, 2), (2, 3), (0, 4), (0, 5), (0, 6)] {
            let right_rule = rules[right].candidate.rule.clone();
            rules[left].overlaps.insert(right_rule);
        }
        rules[3].candidate.outcome = approve_with_reason("other");
        rules[3].candidate.priority = rules[0].candidate.priority;
        rules[4].candidate.outcome = deny();
        rules[4].candidate.priority = rules[0].candidate.priority + 1;

        let analysis = analyze_interactions(&PrecedenceModel::PriorityFirst, rules);
        assert_eq!(analysis.reachability.len(), 7);
        let unreachable = analysis
            .reachability
            .iter()
            .find(|entry| entry.rule.rule().as_str() == "rule.unreachable")
            .expect("unreachable");
        assert_eq!(unreachable.status, ReachabilityStatus::Unreachable);
        assert!(unreachable.proof.is_some());
        assert!(
            analysis
                .reachability
                .iter()
                .filter(|entry| entry.status == ReachabilityStatus::Reachable)
                .all(|entry| entry.witness.is_some())
        );
        assert_eq!(
            analysis
                .overlaps
                .iter()
                .map(|entry| entry.classification)
                .collect::<Vec<_>>(),
            vec![
                OverlapClassification::Conflict,
                OverlapClassification::Compatible,
                OverlapClassification::ExplicitOverride,
                OverlapClassification::Redundant,
                OverlapClassification::ShadowedApproval,
            ]
        );
        assert!(
            analysis
                .overlaps
                .iter()
                .all(|entry| entry.witness.synthetic)
        );
        assert_eq!(
            analysis
                .findings
                .iter()
                .map(|entry| entry.code)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["RUL200", "RUL201", "RUL202", "RUL203", "RUL204",])
        );
    }

    fn rule(id: &str) -> RuleAnalysisInput {
        let qualified = QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new(format!("rule.{id}")).expect("rule"),
        );
        let path = FactPath::from_str("member.active").expect("path");
        let facts = BTreeMap::from([(path.clone(), Value::Boolean(true))]);
        let at = UtcInstant::new(1).expect("instant");
        let database = TimeZoneDatabaseIdentity::new("test", "1").expect("database");
        let witness = minimize_witness(
            id,
            facts,
            at,
            database,
            BTreeSet::from([path.clone()]),
            WitnessClaim::ReachableRule(qualified.clone()),
            |candidate, _, _| candidate.contains_key(&path),
        );
        RuleAnalysisInput {
            candidate: Candidate {
                rule: qualified,
                outcome: approve(),
                priority: 1,
                specificity: 1,
                override_rank: 0,
            },
            satisfiable: Some(true),
            condition_hash: ContentHash::digest(id.as_bytes()),
            overlaps: BTreeSet::new(),
            explicit_override: false,
            undeclared_override: false,
            witness,
        }
    }

    fn reasons() -> Reasons {
        Reasons::new(vec![
            Reason::new(ReasonCode::new("reason").expect("code"), "reason").expect("reason"),
        ])
        .expect("reasons")
    }

    fn approve() -> Outcome {
        Outcome::approve(reasons(), Vec::new())
    }
    fn approve_with_reason(code: &str) -> Outcome {
        Outcome::approve(
            Reasons::new(vec![
                Reason::new(ReasonCode::new(code).expect("code"), code).expect("reason"),
            ])
            .expect("reasons"),
            Vec::new(),
        )
    }
    fn deny() -> Outcome {
        Outcome::deny(reasons(), Vec::new())
    }
}
