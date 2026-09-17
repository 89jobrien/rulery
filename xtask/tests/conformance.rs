//! xtask specification conformance workflow tests.

use std::process::Command;

#[test]
fn conformance_accepts_normative_specification() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("conformance")
        .output()
        .expect("xtask must start");

    assert!(
        output.status.success(),
        "conformance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
