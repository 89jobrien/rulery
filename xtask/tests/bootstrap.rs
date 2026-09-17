//! xtask workspace bootstrap workflow tests.

use std::process::Command;

#[test]
fn bootstrap_check_accepts_scaffolded_workspace() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bootstrap", "--check"])
        .output()
        .expect("xtask must start");

    assert!(
        output.status.success(),
        "bootstrap check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
