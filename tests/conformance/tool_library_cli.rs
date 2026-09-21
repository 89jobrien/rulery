//! End-to-end reproducibility checks for the tool-library explain workflow.

use std::fs;

use rulery::contracts::UtcInstant;

#[test]
fn tool_library_cli_is_reproducible_end_to_end() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tool-library");
    let at = UtcInstant::new(1_789_574_400_000_000_000).expect("instant");
    let first = rulery::tool_library_explain(&root, at).expect("first explain");
    let second = rulery::tool_library_explain(&root, at).expect("second explain");
    assert_eq!(first.json, second.json);
    assert_eq!(first.outcome, rulery::contracts::OutcomeKind::Deny);
    assert_eq!(first.policy_date.to_string(), "2026-09-16");
    assert_eq!(
        first.determining_rule.to_string(),
        "community-tool-library::deny-expired-training"
    );
    assert!(
        String::from_utf8(first.json.clone())
            .expect("json")
            .contains("expired-training")
    );
    assert_ne!(
        first.package_hash,
        rulery::contracts::ContentHash::from_bytes([0; 32])
    );
    assert_ne!(
        first.facts_hash,
        rulery::contracts::ContentHash::from_bytes([0; 32])
    );
    assert_ne!(
        first.trace_hash,
        rulery::contracts::ContentHash::from_bytes([0; 32])
    );
    assert!(!first.timezone_database.implementation().is_empty());
    assert_eq!(
        first.human,
        fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/tool-library-explain.txt")
        )
        .expect("human fixture")
    );
}
