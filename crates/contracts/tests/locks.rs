//! Versioned rulebook lock contract tests.

use rulery_contracts::{
    ContentHash, LanguageVersion, LockedImport, LockedRoot, NormalizedSourceLocation, PackageId,
    RulebookLock, RulebookLockEnvelope, Version,
};

#[test]
fn lock_envelope_requires_sorted_complete_payload() {
    assert!(NormalizedSourceLocation::new(".").is_err());

    let root = LockedRoot::new(
        PackageId::new("root").expect("package"),
        Version::new("0.1.0").expect("version"),
        NormalizedSourceLocation::new("packages/root").expect("source"),
        ContentHash::from_bytes([0; 32]),
    );
    let lock = RulebookLock::new(root, LanguageVersion::V1, vec![]).expect("lock");
    let encoded = serde_json::to_value(RulebookLockEnvelope::from(lock)).expect("lock JSON");
    assert_eq!(encoded["schema"], "rulery.rulebook-lock/v1");
    assert_eq!(encoded["payload"]["imports"], serde_json::json!([]));

    let import = LockedImport::new(
        PackageId::new("shared").expect("package"),
        Version::new("1.0.0").expect("version"),
        NormalizedSourceLocation::new("packages/shared").expect("source"),
        ContentHash::from_bytes([1; 32]),
    );
    let root = LockedRoot::new(
        PackageId::new("root").expect("package"),
        Version::new("0.1.0").expect("version"),
        NormalizedSourceLocation::new("packages/root").expect("source"),
        ContentHash::from_bytes([0; 32]),
    );
    assert!(RulebookLock::new(root, LanguageVersion::V1, vec![import.clone(), import]).is_err());

    let unknown = r#"{"schema":"rulery.rulebook-lock/v1","payload":{"root":{},"language_version":1,"imports":[],"extra":true}}"#;
    assert!(serde_json::from_str::<RulebookLockEnvelope>(unknown).is_err());
}
