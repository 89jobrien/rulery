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

const NETWORK_CAPABLE: &[&str] = &[
    "isahc",
    "attohttpc",
    "aws-sdk",
    "aws-smithy",
    "curl",
    "git2",
    "h2",
    "hyper",
    "hyper-tls",
    "libgit2",
    "mio",
    "native-tls",
    "octocrab",
    "openssl",
    "quinn",
    "reqwest",
    "rustls",
    "smtp",
    "socket2",
    "surf",
    "tokio",
    "tonic",
    "ureq",
];

#[test]
fn resolved_dependency_closure_has_no_network_capable_crate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("readable Cargo.lock");
    for name in NETWORK_CAPABLE {
        let entry = format!("name = \"{name}\"");
        assert!(
            !lock.contains(&entry),
            "network-capable crate resolved into the graph: {name}"
        );
    }
}

#[test]
fn declared_licenses_have_matching_text_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("root Cargo.toml must be readable");

    let declared = declared_licenses(&manifest);
    assert!(
        !declared.is_empty(),
        "no SPDX license expression found in [workspace.package]; this test would pass vacuously"
    );

    for identifier in declared {
        let (file, marker) = license_file(&identifier);
        let path = root.join(file);
        assert!(
            path.is_file(),
            "{identifier} is declared in Cargo.toml but {} is missing",
            path.display()
        );
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            text.contains(marker),
            "{} does not contain the {identifier} text",
            path.display()
        );
    }
}

/// Reads the SPDX license expression from `[workspace.package]`.
///
/// This is a deliberate narrow line scan rather than a TOML parse, because the workspace has no
/// TOML dependency and a test is not a reason to add one. It fails loudly rather than silently
/// passing: a manifest that stops using `license = "..."` leaves `declared` empty, and a manifest
/// that spells it differently leaves the SPDX identifiers unrecognised.
fn declared_licenses(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .filter_map(|line| line.trim().strip_prefix("license = \""))
        .filter_map(|line| line.strip_suffix('"'))
        .flat_map(|expression| expression.split(" OR "))
        .map(|identifier| identifier.trim().to_owned())
        .filter(|identifier| !identifier.is_empty())
        .collect()
}

/// Maps one SPDX identifier to the repository file and a marker its text must contain.
fn license_file(identifier: &str) -> (&'static str, &'static str) {
    match identifier {
        "MIT" => (
            "LICENSE-MIT",
            "Permission is hereby granted, free of charge",
        ),
        "Apache-2.0" => (
            "LICENSE-APACHE",
            "Licensed under the Apache License, Version 2.0",
        ),
        other => panic!("no license text file is defined for SPDX identifier `{other}`"),
    }
}

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
