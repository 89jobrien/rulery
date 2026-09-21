use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use rulery::analysis::{
    AnalysisBudget, BudgetResult, StructuralChange, WitnessClaim, minimize_witness,
};
use rulery::contracts::{
    EscalationId, FactPath, FactValidationError, Outcome, PackageId, QualifiedRuleId, Reason,
    ReasonCode, Reasons, RequiredFacts, RuleId, SourceFile, SourceId, SourceKey, SourceMap,
    SourcePath, StableId, TimeZoneDatabaseIdentity, UtcInstant, Value,
};
use rulery::diagnostics::{FixApplicability, SuggestedFix, TextEdit};
use rulery::emit::{
    MarkdownRenderer, ProjectionCapabilities, ProjectionFeature, ProjectionRule,
    validate_projection,
};
use rulery::engine::{
    Candidate, EvidenceClass, InvalidFactStrategy, MissingFactStrategy, OperandState,
    PrecedenceModel, PredicateObservation, PresencePredicate, Truth, apply_strategies,
    evaluate_presence, select_candidates, should_use_default,
};

#[test]
fn normative_safety_properties_hold_across_pipeline() {
    let path = FactPath::from_str("member.email").expect("path");
    let observation = PredicateObservation {
        path: path.clone(),
        truth: Truth::Unknown,
        evidence: EvidenceClass::Absent,
        priority: 1,
        specificity: 1,
        override_rank: 0,
    };
    let preserved = apply_strategies(
        std::slice::from_ref(&observation),
        &MissingFactStrategy::PreserveUnknown,
        &InvalidFactStrategy::PreserveInvalid,
    )
    .expect("preserve");
    assert_eq!(preserved.adjusted_truths, vec![Truth::Unknown]);
    let closed = apply_strategies(
        &[observation],
        &MissingFactStrategy::ClosedWorldFalse,
        &InvalidFactStrategy::PreserveInvalid,
    )
    .expect("closed");
    assert_eq!(closed.adjusted_truths, vec![Truth::False]);

    let malformed_error = FactValidationError::OutOfRange;
    assert_eq!(
        evaluate_presence(PresencePredicate::IsAbsent, OperandState::Null),
        Truth::False
    );
    assert_eq!(
        evaluate_presence(
            PresencePredicate::IsAbsent,
            OperandState::Malformed(&malformed_error)
        ),
        Truth::False
    );
    assert!(should_use_default(false, false));
    assert!(!should_use_default(false, true));

    assert!(Reasons::new(Vec::new()).is_err());
    assert!(RequiredFacts::new(BTreeSet::new()).is_err());
    let reasons = reasons("allowed");
    assert_eq!(
        Outcome::approve(reasons.clone(), Vec::new()).kind(),
        rulery::contracts::OutcomeKind::Approve
    );
    assert_eq!(
        Outcome::deny(reasons.clone(), Vec::new()).kind(),
        rulery::contracts::OutcomeKind::Deny
    );
    assert_eq!(
        Outcome::escalate(
            EscalationId::new("review").expect("destination"),
            reasons,
            Vec::new()
        )
        .kind(),
        rulery::contracts::OutcomeKind::Escalate
    );

    let left = candidate("rule.z");
    let right = candidate("rule.a");
    let forward = select_candidates(
        &PrecedenceModel::PriorityFirst,
        vec![left.clone(), right.clone()],
    )
    .expect("selection");
    let reverse =
        select_candidates(&PrecedenceModel::PriorityFirst, vec![right, left]).expect("selection");
    assert_eq!(forward.outcome, reverse.outcome);
    assert_eq!(forward.determining_rules, reverse.determining_rules);

    let semantic_inputs = BTreeSet::from([
        StructuralChange::Default,
        StructuralChange::Precedence,
        StructuralChange::MissingStrategy,
        StructuralChange::InvalidStrategy,
        StructuralChange::Timezone,
        StructuralChange::Expiry,
        StructuralChange::Vocabulary,
        StructuralChange::ImportIntegrity,
        StructuralChange::Reasons,
        StructuralChange::RequiredFacts,
        StructuralChange::Actions,
    ]);
    assert_eq!(semantic_inputs.len(), 11);

    let at = UtcInstant::new(1).expect("instant");
    let database = TimeZoneDatabaseIdentity::new("test", "1").expect("database");
    let witness = minimize_witness(
        "safety",
        BTreeMap::from([(path.clone(), Value::Text("x".to_owned()))]),
        at,
        database,
        BTreeSet::from([path.clone()]),
        WitnessClaim::Uncovered,
        |candidate, _, _| candidate.contains_key(&path),
    );
    assert!(witness.synthetic);

    let markdown =
        MarkdownRenderer.render_truths(&[("unknown", Truth::Unknown), ("invalid", Truth::Invalid)]);
    assert!(markdown.contains("Unknown") && markdown.contains("Invalid"));

    let rule = ProjectionRule {
        rule: qualified("rule.loss"),
        cells: Vec::new(),
        outcome: rulery::contracts::OutcomeKind::Escalate,
        span: span(),
        truth_states: vec![Truth::Unknown, Truth::Invalid],
        has_actions: true,
        has_reasons: true,
    };
    let losses = validate_projection(
        &[rule],
        ProjectionCapabilities {
            escalation: false,
            information_request: false,
            uncertainty: false,
            actions: false,
            reasons: false,
        },
    );
    assert!(
        losses
            .iter()
            .any(|loss| loss.feature == ProjectionFeature::Escalation)
    );
    assert!(
        losses
            .iter()
            .any(|loss| loss.feature == ProjectionFeature::Unknown)
    );
    assert!(
        losses
            .iter()
            .any(|loss| loss.feature == ProjectionFeature::Invalid)
    );

    let map = source_map();
    let overlapping = SuggestedFix::new(
        StableId::new("fix").expect("id"),
        "fix",
        FixApplicability::MachineApplicable,
        vec![
            TextEdit::new(map.span(SourceKey::new(1), 0, 2).expect("span"), "a"),
            TextEdit::new(map.span(SourceKey::new(1), 1, 3).expect("span"), "b"),
        ],
    );
    assert!(overlapping.is_err());
    assert!(SourcePath::new("../escape").is_err());

    let mut budget = AnalysisBudget::<u8, bool>::new(0);
    assert_eq!(
        budget.resolve(1, || true),
        BudgetResult::Inconclusive {
            examined: 0,
            limit: 0
        }
    );
    assert_eq!(
        rulery::AssemblyError::ResourceLimit {
            kind: "package count",
            max: 1
        }
        .diagnostic_code(),
        Some(rulery::diagnostics::DiagnosticCode::IMPORT_RESOURCE_LIMIT)
    );
}

fn candidate(rule: &str) -> Candidate {
    Candidate {
        rule: qualified(rule),
        outcome: Outcome::approve(reasons("allowed"), Vec::new()),
        priority: 1,
        specificity: 1,
        override_rank: 0,
    }
}

fn reasons(code: &str) -> Reasons {
    Reasons::new(vec![
        Reason::new(ReasonCode::new(code).expect("code"), code).expect("reason"),
    ])
    .expect("reasons")
}

fn qualified(rule: &str) -> QualifiedRuleId {
    QualifiedRuleId::new(
        PackageId::new("pkg.main").expect("package"),
        RuleId::new(rule).expect("rule"),
    )
}

fn source_map() -> SourceMap {
    let mut map = SourceMap::new();
    map.insert(
        SourceKey::new(1),
        SourceFile::new(
            SourceId::new("source.main").expect("source"),
            SourcePath::new("main.yaml").expect("path"),
            Arc::<str>::from("abc"),
        ),
    )
    .expect("insert");
    map
}

fn span() -> rulery::contracts::Span {
    source_map().span(SourceKey::new(1), 0, 1).expect("span")
}
