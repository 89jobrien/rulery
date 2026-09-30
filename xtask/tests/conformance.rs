//! xtask specification conformance workflow tests.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::{ProcessOutcome, ProcessRunner};

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
    let checks = report
        .failures
        .iter()
        .map(|failure| failure.check)
        .collect::<Vec<_>>();
    assert_eq!(
        checks
            .iter()
            .copied()
            .filter(|check| *check != "gates")
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
            "hash-vectors",
        ]
    );
    assert_eq!(
        checks.iter().filter(|check| **check == "gates").count(),
        xtask::required_gate_bullets().len(),
        "each absent required gate is reported on its own, so a report names every missing bullet"
    );
    assert_eq!(
        checks.last(),
        Some(&"gates"),
        "gate failures come last so the ordered report stays stable as bullets are added"
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

/// Builds a specification whose `Required conformance gates` section holds exactly `bullets`.
fn gates_specification(bullets: &[&str]) -> String {
    let mut text = String::from(
        "### Required conformance gates\n\nA conforming implementation MUST pass:\n\n",
    );
    for bullet in bullets {
        writeln!(text, "- {bullet}").expect("string write");
    }
    text
}

/// Every failure the gate-bullet check reported.
fn gate_failures(report: &xtask::ConformanceReport) -> Vec<String> {
    report
        .failures
        .iter()
        .filter(|failure| failure.check == "gates")
        .map(|failure| failure.message.clone())
        .collect()
}

#[test]
fn conformance_rejects_a_specification_missing_a_required_gate_bullet() {
    let runner = FakeRunner { result: Ok(true) };
    let scratch = Path::new(".ctx/_WORKING_DIR/xtask-conformance");
    let bullets = xtask::required_gate_bullets();
    assert!(
        bullets.len() >= 14,
        "the specification declares more required gates than this test enumerates"
    );

    let complete = xtask::validate_specification(&gates_specification(bullets), scratch, &runner)
        .expect("report");
    assert_eq!(gate_failures(&complete), Vec::<String>::new());

    for (index, bullet) in bullets.iter().enumerate() {
        let without = bullets
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, other)| *other)
            .collect::<Vec<_>>();
        let report =
            xtask::validate_specification(&gates_specification(&without), scratch, &runner)
                .expect("report");
        assert_eq!(
            gate_failures(&report),
            vec![format!("required conformance gate is missing: {bullet}")],
            "removing bullet {index} must be reported, and only that bullet"
        );
    }
}

struct FakeRunner {
    result: Result<bool, String>,
}
impl ProcessRunner for FakeRunner {
    fn rustfmt_check(&self, _: &Path) -> Result<bool, String> {
        self.result.clone()
    }

    fn cargo_check(&self, _: &Path, _: &str) -> Result<ProcessOutcome, String> {
        Ok(ProcessOutcome::Success)
    }

    fn cargo_run(&self, _: &Path, _: &str) -> Result<ProcessOutcome, String> {
        Ok(ProcessOutcome::Success)
    }

    fn cargo_fmt_check(&self, _: &Path) -> Result<bool, String> {
        Ok(true)
    }
}

/// One `cargo check` request captured by [`RecordingRunner`].
#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckRequest {
    manifest: PathBuf,
    features: String,
}

/// Answers each recorded `cargo check` from a scripted outcome.
struct RecordingRunner {
    requests: std::sync::Mutex<Vec<CheckRequest>>,
    outcomes: Vec<Result<ProcessOutcome, String>>,
}

impl RecordingRunner {
    fn with_outcomes(outcomes: Vec<Result<ProcessOutcome, String>>) -> Self {
        Self {
            requests: std::sync::Mutex::new(Vec::new()),
            outcomes,
        }
    }

    fn requests(&self) -> Vec<CheckRequest> {
        self.requests.lock().expect("recording lock").clone()
    }
}

impl ProcessRunner for RecordingRunner {
    fn rustfmt_check(&self, _: &Path) -> Result<bool, String> {
        Ok(true)
    }

    fn cargo_check(&self, manifest: &Path, features: &str) -> Result<ProcessOutcome, String> {
        let index = {
            let mut requests = self.requests.lock().expect("recording lock");
            let index = requests.len();
            requests.push(CheckRequest {
                manifest: manifest.to_path_buf(),
                features: features.to_owned(),
            });
            index
        };
        self.outcomes
            .get(index)
            .cloned()
            .unwrap_or(Ok(ProcessOutcome::Success))
    }

    fn cargo_run(&self, _: &Path, _: &str) -> Result<ProcessOutcome, String> {
        Ok(ProcessOutcome::Success)
    }

    fn cargo_fmt_check(&self, _: &Path) -> Result<bool, String> {
        Ok(true)
    }
}

#[test]
fn macro_hygiene_compiles_the_fixture_with_default_features_and_macros() {
    let runner = RecordingRunner::with_outcomes(vec![
        Ok(ProcessOutcome::Success),
        Ok(ProcessOutcome::Success),
    ]);

    xtask::check_macro_hygiene(workspace_root(), &runner).expect("hygiene fixture must compile");

    let manifest = workspace_root().join("examples/macro-hygiene/Cargo.toml");
    assert_eq!(
        runner.requests(),
        vec![
            CheckRequest {
                manifest: manifest.clone(),
                features: String::new(),
            },
            CheckRequest {
                manifest,
                features: "macros".to_owned(),
            },
        ],
        "the specification requires the renamed fixture with default features and with macros"
    );
}

#[test]
fn macro_hygiene_reports_the_feature_set_that_failed_to_compile() {
    let runner = RecordingRunner::with_outcomes(vec![
        Ok(ProcessOutcome::Success),
        Ok(ProcessOutcome::Failed(
            "error[E0425]: cannot find type `NoSuchType` in module `rlry::__private`".to_owned(),
        )),
    ]);
    let error = xtask::check_macro_hygiene(workspace_root(), &runner)
        .expect_err("a failing feature set must fail the gate")
        .to_string();
    assert!(error.contains("examples/macro-hygiene"), "{error}");
    assert!(error.contains("features `macros`"), "{error}");
    assert!(
        error.contains("E0425"),
        "cargo's own diagnostics must survive: {error}"
    );

    let runner = RecordingRunner::with_outcomes(vec![Err("cargo is not installed".to_owned())]);
    let error = xtask::check_macro_hygiene(workspace_root(), &runner)
        .expect_err("a runner failure must not be reported as a hygiene failure")
        .to_string();
    assert!(error.contains("cargo is not installed"), "{error}");
    assert!(!error.contains("does not compile"), "{error}");
}

const REQUIREMENT_IDS: &[&str] = &[
    "V01", "V02", "V03", "V04", "V05", "V06", "V07", "V08", "V09", "V10", "V11", "V12", "V13",
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
