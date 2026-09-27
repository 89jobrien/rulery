//! xtask command-line contract tests.

use std::process::Command;

#[test]
fn help_lists_complex_workflows() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("--help")
        .output()
        .expect("xtask must start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help output must be UTF-8");
    for command in ["bootstrap", "conformance", "architecture", "verify"] {
        assert!(stdout.contains(command), "help omitted {command}");
    }
}
