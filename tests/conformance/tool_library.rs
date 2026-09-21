use std::fs;

use rulery::contracts::PackagePath;
use rulery::store::{FilesystemPackageStore, PackageStore};

#[test]
fn tool_library_fixture_matches_normative_sources() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tool-library");
    let manifest = yaml(root.join("rulery.yaml"));
    assert_eq!(manifest["package"]["id"], "community-tool-library");
    assert_eq!(manifest["package"]["version"], "0.1.0");
    assert_eq!(manifest["semantics"]["timezone"], "America/New_York");
    assert_eq!(manifest["imports"].as_sequence().expect("imports").len(), 0);

    let vocabulary = yaml(root.join("vocabulary.yaml"));
    assert_eq!(vocabulary["roots"].as_mapping().expect("roots").len(), 2);
    assert_eq!(vocabulary["types"].as_mapping().expect("types").len(), 5);
    assert!(vocabulary["terms"]["current-training"].is_mapping());

    let actions = yaml(root.join("actions.yaml"));
    assert_eq!(actions["actions"].as_mapping().expect("actions").len(), 2);

    let rules = yaml(root.join("rules/checkout.yaml"));
    let rule_ids = rules["rules"]
        .as_sequence()
        .expect("rules")
        .iter()
        .map(|rule| rule["id"].as_str().expect("rule id"))
        .collect::<Vec<_>>();
    assert_eq!(
        rule_ids,
        vec![
            "deny-suspended-member",
            "request-training-record",
            "deny-expired-training",
            "approve-qualified-checkout",
        ]
    );

    let scenario = yaml(root.join("scenarios/expired-training-is-denied.yaml"));
    assert_eq!(scenario["expect"]["outcome"], "deny");
    assert_eq!(
        scenario["expect"]["determining_rules"][0]["rule"],
        "deny-expired-training"
    );
    let case = yaml(root.join("cases/expired-training.yaml"));
    assert_eq!(case["member"]["training"]["valid-until"], "2026-09-15");
    assert_eq!(case["tool"]["category"], "power-tool");

    let package_path = PackagePath::new(root).expect("package path");
    let store = FilesystemPackageStore::default();
    let loaded = store.load_source(&package_path).expect("source bundle");
    let lock = store
        .load_lock(&package_path)
        .expect("load lock")
        .expect("lock exists");
    assert!(lock.payload().imports().is_empty());
    assert_eq!(
        lock.payload().root().content_hash().as_bytes(),
        loaded.integrity().as_bytes()
    );
}

fn yaml(path: std::path::PathBuf) -> serde_yaml::Value {
    let bytes = fs::read(path).expect("fixture file");
    serde_yaml::from_slice(&bytes).expect("valid YAML")
}
