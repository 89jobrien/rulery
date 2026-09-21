//! xtask specification conformance workflow tests.

use std::path::Path;
use std::process::Command;

use xtask::ProcessRunner;

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

#[test]
fn conformance_report_collects_every_specification_failure() {
    let invalid = "# Specification\n[bad](#missing)\nTODO\n```yaml\n[\n```\n```json\n{\n```\n```rust\nfn  main( ){println!(\"x\");}\n```\n";
    let report = xtask::validate_specification(
        invalid,
        Path::new(".ctx/_WORKING_DIR/xtask-conformance"),
        &FakeRunner { result: Ok(false) },
    )
    .expect("report");
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.check)
            .collect::<Vec<_>>(),
        vec![
            "headings",
            "links",
            "incomplete",
            "yaml",
            "json",
            "rustfmt",
            "registry",
            "schemas",
            "hash-vectors"
        ]
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("root");
    let valid = std::fs::read_to_string(root.join("docs/specification.md")).expect("specification");
    let valid_report = xtask::validate_specification(
        &valid,
        Path::new(".ctx/_WORKING_DIR/xtask-conformance"),
        &FakeRunner { result: Ok(true) },
    )
    .expect("valid report");
    assert!(valid_report.failures.is_empty());

    assert!(
        xtask::validate_specification(
            "```rust\nfn main() {}\n```",
            Path::new("outside"),
            &FakeRunner { result: Ok(true) }
        )
        .is_err()
    );
    assert!(
        xtask::validate_specification(
            "```rust\nfn main() {}\n```",
            Path::new(".ctx/_WORKING_DIR/xtask-conformance"),
            &FakeRunner {
                result: Err("missing rustfmt".to_owned())
            }
        )
        .is_err()
    );
}

struct FakeRunner {
    result: Result<bool, String>,
}
impl ProcessRunner for FakeRunner {
    fn rustfmt_check(&self, _: &Path) -> Result<bool, String> {
        self.result.clone()
    }
}
