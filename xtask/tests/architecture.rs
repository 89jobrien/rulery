//! xtask architecture workflow tests.

use std::process::Command;

#[test]
fn architecture_accepts_scaffolded_workspace() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("architecture")
        .output()
        .expect("xtask must start");

    assert!(
        output.status.success(),
        "architecture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn workspace_model_is_single_architecture_authority() {
    let model = xtask::workspace_model();
    assert_eq!(model.root, ".");
    assert_eq!(model.crates.len(), 15);
    assert_eq!(
        model
            .crates
            .iter()
            .filter(|spec| spec.path.starts_with("crates/"))
            .count(),
        13
    );
    assert!(model.crates.iter().any(|spec| spec.name == "rulery"
        && spec.path == "."
        && spec.target == xtask::TargetKind::Library));
    assert!(model.crates.iter().any(|spec| spec.name == "xtask"
        && spec.path == "xtask"
        && spec.target == xtask::TargetKind::Binary));
    assert!(!model.crates.iter().any(|spec| spec.name.contains("lsp")));
    for spec in model.crates {
        assert_eq!(xtask::model_manifest(spec.name), Some(spec.manifest));
        assert_eq!(xtask::model_target(spec.name), Some(spec.target));
        assert_eq!(
            xtask::model_dependencies(spec.name),
            Some(spec.dependencies.allowed)
        );
    }
}
