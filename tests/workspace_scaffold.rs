//! Workspace scaffold conformance tests.

use std::path::Path;

const CRATES: &[(&str, &str)] = &[
    ("contracts", "rulery-contracts"),
    ("diagnostics", "rulery-diagnostics"),
    ("syntax", "rulery-syntax"),
    ("vocabulary", "rulery-vocabulary"),
    ("ir", "rulery-ir"),
    ("compiler", "rulery-compiler"),
    ("engine", "rulery-engine"),
    ("analysis", "rulery-analysis"),
    ("scenarios", "rulery-scenarios"),
    ("emit", "rulery-emit"),
    ("store", "rulery-store"),
    ("cli", "rulery-cli"),
    ("macros", "rulery-macros"),
];

#[test]
fn workspace_contains_approved_crates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root_manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("root Cargo.toml must be readable");
    assert!(root_manifest.contains("[workspace]"));
    assert!(root_manifest.contains("members = [\"crates/*\", \"xtask\"]"));
    assert!(root.join("src/lib.rs").is_file());
    assert!(!root.join("src/main.rs").exists());
    for directory in [
        "examples/tool-library",
        "docs/adr",
        "docs/language",
        "docs/semantics",
        "tests/conformance",
    ] {
        assert!(
            root.join(directory).is_dir(),
            "missing directory: {directory}"
        );
    }

    for (directory, package) in CRATES {
        let manifest = root.join("crates").join(directory).join("Cargo.toml");
        assert!(
            manifest.is_file(),
            "missing manifest: {}",
            manifest.display()
        );

        let contents = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest.display()));
        assert!(
            contents.contains(&format!("name = \"{package}\"")),
            "{} does not declare package {package}",
            manifest.display()
        );
    }

    let xtask_manifest = root.join("xtask").join("Cargo.toml");
    assert!(
        xtask_manifest.is_file(),
        "missing manifest: {}",
        xtask_manifest.display()
    );
    let xtask_contents = std::fs::read_to_string(&xtask_manifest)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", xtask_manifest.display()));
    assert!(
        xtask_contents.contains("name = \"xtask\""),
        "{} does not declare package xtask",
        xtask_manifest.display()
    );
}
