//! Value, fact, action, and outcome contract tests.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{
    CaseFacts, EscalationId, FactPath, FactRootId, FactState, OutcomeKind, OutcomeTemplate, Reason,
    ReasonCode, Reasons, RequiredFacts, Value,
};

#[test]
fn outcomes_require_reasons_and_kind_specific_data() {
    let values = [
        Value::Null,
        Value::Boolean(true),
        Value::Integer(42),
        Value::Text("safe".to_owned()),
        Value::List(vec![Value::Integer(1), Value::Integer(2)]),
        Value::Record(BTreeMap::new()),
    ];
    for value in values {
        let encoded = serde_json::to_string(&value).expect("serialize value");
        assert_eq!(
            serde_json::from_str::<Value>(&encoded).expect("value"),
            value
        );
    }

    let root = FactRootId::new("member").expect("root ID");
    let facts = CaseFacts::new(BTreeMap::from([(root.clone(), Value::Null)]));
    assert!(matches!(facts.root(&root), FactState::Null));
    assert!(matches!(
        facts.root(&FactRootId::new("tool").expect("root ID")),
        FactState::Absent
    ));

    assert!(Reasons::new(Vec::new()).is_err());
    assert!(RequiredFacts::new(BTreeSet::new()).is_err());
    assert!(Reason::new(ReasonCode::new("allowed").expect("reason code"), "").is_err());

    let reasons = Reasons::new(vec![
        Reason::new(
            ReasonCode::new("allowed").expect("reason code"),
            "Policy permits this action.",
        )
        .expect("reason"),
    ])
    .expect("reasons");
    let requested = RequiredFacts::new(BTreeSet::from(["member.training"
        .parse::<FactPath>()
        .expect("fact path")]))
    .expect("required facts");

    assert_eq!(
        OutcomeTemplate::approve(reasons.clone(), vec![]).kind(),
        OutcomeKind::Approve
    );
    assert_eq!(
        OutcomeTemplate::escalate(
            EscalationId::new("safety-team").expect("destination"),
            reasons.clone(),
            vec![],
        )
        .kind(),
        OutcomeKind::Escalate
    );
    assert_eq!(
        OutcomeTemplate::request_information(requested, reasons, vec![]).kind(),
        OutcomeKind::RequestInformation
    );
}
