//! Conformance tests for the optional `RuleFacts` derive.
//!
//! `crates/macros` tests the derive at the token level: valid input expands and invalid input is
//! rejected. Nothing compiled the generated code, so the promises its documentation makes about the
//! resulting conversion were unverified, and a derive that expanded to uncompilable code would have
//! passed the whole suite.
//!
//! The compile-time promises — a required root, a required path per field, and rejection of
//! duplicate, nested, unknown, generic, and tuple shapes — stay in `crates/macros`, where they
//! are asserted on `expand`. Renamed-dependency hygiene needs a downstream crate and lives with
//! the workspace's hygiene fixture instead.

#![cfg(feature = "macros")]

use std::collections::BTreeMap;

use rulery::RuleFacts;
use rulery::contracts::{CaseFacts, FactRootId, FactState, StableId, Value};

/// Every supported scalar field type, so the derive's conversion for each is compiled.
#[derive(RuleFacts)]
#[rulery(root = "member")]
struct MemberFacts {
    #[rulery(path = "active")]
    active: bool,
    #[rulery(path = "name")]
    name: String,
    #[rulery(path = "age")]
    age: i64,
    #[rulery(path = "score")]
    score: Value,
}

/// Optional fields, which the documentation says are omitted rather than nulled.
#[derive(RuleFacts)]
#[rulery(root = "member")]
struct OptionalFacts {
    #[rulery(path = "nickname")]
    nickname: Option<String>,
    #[rulery(path = "tier")]
    tier: Option<i64>,
    #[rulery(path = "active")]
    active: Option<bool>,
}

/// Returns the record fields under the `member` root, or panics with what was found instead.
fn member_fields(facts: &CaseFacts) -> &BTreeMap<StableId, Value> {
    let FactState::Valid(Value::Record(fields)) =
        facts.root(&FactRootId::new("member").expect("root"))
    else {
        panic!("member root is not a valid record");
    };
    fields
}

fn field<'facts>(fields: &'facts BTreeMap<StableId, Value>, name: &str) -> Option<&'facts Value> {
    fields.get(&StableId::new(name).expect("stable id"))
}

#[test]
fn rule_facts_derive_converts_a_record_into_case_facts() {
    let facts = CaseFacts::try_from(MemberFacts {
        active: true,
        name: "Ada".to_owned(),
        age: 41,
        score: Value::Integer(9),
    })
    .expect("conversion");

    let fields = member_fields(&facts);
    assert_eq!(
        field(fields, "active"),
        Some(&Value::Boolean(true)),
        "bool field did not convert"
    );
    assert_eq!(
        field(fields, "name"),
        Some(&Value::Text("Ada".to_owned())),
        "String field did not convert"
    );
    assert_eq!(
        field(fields, "age"),
        Some(&Value::Integer(41)),
        "i64 field did not convert"
    );
    assert_eq!(
        field(fields, "score"),
        Some(&Value::Integer(9)),
        "Value field did not pass through unchanged"
    );
    assert_eq!(fields.len(), 4, "unexpected fields: {fields:?}");
}

#[test]
fn rule_facts_derive_omits_none_fields_rather_than_nulling_them() {
    let absent = CaseFacts::try_from(OptionalFacts {
        nickname: None,
        tier: None,
        active: None,
    })
    .expect("conversion");
    let fields = member_fields(&absent);
    assert!(
        fields.is_empty(),
        "Option::None must omit the field rather than insert null: {fields:?}"
    );

    let present = CaseFacts::try_from(OptionalFacts {
        nickname: Some("Ada".to_owned()),
        tier: Some(2),
        active: Some(true),
    })
    .expect("conversion");
    let fields = member_fields(&present);
    assert_eq!(
        field(fields, "nickname"),
        Some(&Value::Text("Ada".to_owned())),
        "Option<String> did not convert when present"
    );
    assert_eq!(
        field(fields, "tier"),
        Some(&Value::Integer(2)),
        "Option<i64> did not convert when present"
    );
    assert_eq!(
        field(fields, "active"),
        Some(&Value::Boolean(true)),
        "Option<bool> did not convert when present"
    );
    assert_eq!(fields.len(), 3, "unexpected fields: {fields:?}");
}
