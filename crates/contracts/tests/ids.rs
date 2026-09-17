//! Stable identifier contract tests.

use std::str::FromStr;

use rulery_contracts::{
    ActionId, DecisionId, EscalationId, FactRootId, PackageId, PredicateId, QualifiedRuleId,
    ReasonCode, RuleId, ScenarioId, SourceId, StableId, TypeId,
};

#[test]
fn stable_ids_enforce_wire_grammar() {
    for valid in [
        "a".to_owned(),
        "a".repeat(128),
        "a-b".to_owned(),
        "a_b".to_owned(),
        "a.b".to_owned(),
        "a0".to_owned(),
    ] {
        assert_eq!(
            StableId::from_str(&valid).expect("valid ID").as_str(),
            valid
        );
    }

    for invalid in [
        String::new(),
        "a".repeat(129),
        "Upper".to_owned(),
        "-leading".to_owned(),
        "trailing-".to_owned(),
        "has:colon".to_owned(),
        String::from("caf\u{e9}"),
    ] {
        assert!(
            StableId::from_str(&invalid).is_err(),
            "accepted {invalid:?}"
        );
    }

    macro_rules! assert_typed_id {
        ($type:ty) => {{
            let id: $type = serde_json::from_str("\"valid-id\"").expect("typed ID");
            assert_eq!(id.as_str(), "valid-id");
            assert!(serde_json::from_str::<$type>("\"INVALID\"").is_err());
        }};
    }

    assert_typed_id!(PackageId);
    assert_typed_id!(DecisionId);
    assert_typed_id!(RuleId);
    assert_typed_id!(TypeId);
    assert_typed_id!(PredicateId);
    assert_typed_id!(ActionId);
    assert_typed_id!(ScenarioId);
    assert_typed_id!(EscalationId);
    assert_typed_id!(SourceId);
    assert_typed_id!(FactRootId);
    assert_typed_id!(ReasonCode);

    let qualified = QualifiedRuleId::from_str("community::rule").expect("qualified ID");
    assert_eq!(qualified.package().as_str(), "community");
    assert_eq!(qualified.rule().as_str(), "rule");
    assert_eq!(qualified.to_string(), "community::rule");
    assert_eq!(
        serde_json::to_string(&qualified).expect("serialize qualified ID"),
        "\"community::rule\""
    );
}
