//! xtask specification conformance workflow tests.

use std::fmt::Write as _;
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

const REQUIREMENT_IDS: &[&str] = &[
    "V01", "V02", "V03", "V04", "V05", "V06", "V07", "V08", "V09", "V10", "V11", "V12",
];

const STATIC_CHECK_IDS: &[&str] = &[
    "S01", "S02", "S03", "S04", "S05", "S06", "S07", "S08", "S09",
];

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("root")
}

fn traceability(rows: &[(&str, &str)]) -> String {
    let mut text = String::from(
        "### Versioned requirement traceability\n\n\
         | ID | Requirement | Verification test | Status |\n\
         | --- | --- | --- | --- |\n",
    );
    for (id, verification) in rows {
        writeln!(text, "| {id} | statement | `{verification}` | satisfied |")
            .expect("string write");
    }
    text.push_str("\n## Blueprint phases\n");
    text
}

fn satisfied_rows(verification: &str) -> Vec<(String, String)> {
    REQUIREMENT_IDS
        .iter()
        .chain(STATIC_CHECK_IDS.iter())
        .map(|id| ((*id).to_owned(), verification.to_owned()))
        .collect()
}

#[test]
fn requirements_accept_a_fully_satisfied_and_verified_specification() {
    let rows = satisfied_rows("truth_tables_match_all_36_cells");
    let borrowed: Vec<(&str, &str)> = rows
        .iter()
        .map(|(id, verification)| (id.as_str(), verification.as_str()))
        .collect();
    assert!(xtask::check_requirements(workspace_root(), &traceability(&borrowed)).is_ok());
}

#[test]
fn requirements_reject_an_unverified_or_unsatisfied_declaration() {
    let rows = satisfied_rows("truth_tables_match_all_36_cells");
    let borrowed: Vec<(&str, &str)> = rows
        .iter()
        .map(|(id, verification)| (id.as_str(), verification.as_str()))
        .collect();

    let missing_test =
        traceability(&borrowed).replace("truth_tables_match_all_36_cells", "no_such_test_exists");
    let error = xtask::check_requirements(workspace_root(), &missing_test)
        .expect_err("missing test must fail")
        .to_string();
    assert!(error.contains("no_such_test_exists"), "{error}");

    let unsatisfied = traceability(&borrowed).replace("| satisfied |", "| blocked |");
    let error = xtask::check_requirements(workspace_root(), &unsatisfied)
        .expect_err("blocked requirement must fail")
        .to_string();
    assert!(error.contains("V01 is blocked"), "{error}");

    let mut incomplete = satisfied_rows("truth_tables_match_all_36_cells");
    incomplete.retain(|(id, _)| id != "S05");
    let trimmed: Vec<(&str, &str)> = incomplete
        .iter()
        .map(|(id, verification)| (id.as_str(), verification.as_str()))
        .collect();
    let error = xtask::check_requirements(workspace_root(), &traceability(&trimmed))
        .expect_err("omitted requirement must fail")
        .to_string();
    assert!(error.contains("S05 is not declared"), "{error}");
}

#[test]
fn normative_specification_satisfies_every_declared_requirement() {
    let specification = std::fs::read_to_string(workspace_root().join("docs/specification.md"))
        .expect("specification");

    if let Err(error) = xtask::check_requirements(workspace_root(), &specification) {
        panic!("the normative specification no longer satisfies its requirements: {error}");
    }

    for id in REQUIREMENT_IDS.iter().chain(STATIC_CHECK_IDS.iter()) {
        assert!(
            specification.contains(&format!("| {id} |")),
            "{id} is missing from the traceability tables"
        );
    }
}
