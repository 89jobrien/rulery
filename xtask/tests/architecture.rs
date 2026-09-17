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
