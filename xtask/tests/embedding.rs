//! Consumer embedding gate tests.
//!
//! The gate is only trustworthy if it demonstrably runs the fixture, so these tests assert the
//! process requests it makes rather than re-asserting what the fixture itself checks. What the
//! fixture proves lives in the fixture; what the gate must not get wrong lives here.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use xtask::{ProcessOutcome, ProcessRunner, check_embedding};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .to_path_buf()
}

/// One `cargo run` request captured by [`RunRecorder`].
#[derive(Clone, Debug, Eq, PartialEq)]
struct RunRequest {
    manifest: PathBuf,
    features: String,
}

/// Answers each recorded `cargo run` from a scripted outcome.
struct RunRecorder {
    requests: Mutex<Vec<RunRequest>>,
    outcomes: Vec<Result<ProcessOutcome, String>>,
    formatted: bool,
}

impl RunRecorder {
    fn with_outcomes(outcomes: Vec<Result<ProcessOutcome, String>>) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            outcomes,
            formatted: true,
        }
    }

    fn unformatted() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            outcomes: Vec::new(),
            formatted: false,
        }
    }

    fn requests(&self) -> Vec<RunRequest> {
        self.requests.lock().expect("recording lock").clone()
    }
}

impl ProcessRunner for RunRecorder {
    fn rustfmt_check(&self, _: &Path) -> Result<bool, String> {
        Ok(true)
    }

    fn cargo_check(&self, _: &Path, _: &str) -> Result<ProcessOutcome, String> {
        Ok(ProcessOutcome::Success)
    }

    fn cargo_run(&self, manifest: &Path, features: &str) -> Result<ProcessOutcome, String> {
        let index = {
            let mut requests = self.requests.lock().expect("recording lock");
            let index = requests.len();
            requests.push(RunRequest {
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

    fn cargo_fmt_check(&self, _: &Path) -> Result<bool, String> {
        Ok(self.formatted)
    }
}

#[test]
fn embedding_fixture_is_run_with_default_features_and_macros() {
    let runner = RunRecorder::with_outcomes(vec![
        Ok(ProcessOutcome::Success),
        Ok(ProcessOutcome::Success),
    ]);

    check_embedding(&workspace_root(), &runner)
        .expect("embedding fixture must satisfy the contract");

    let manifest = workspace_root().join("examples/embedding-fixture/Cargo.toml");
    assert_eq!(
        runner.requests(),
        vec![
            RunRequest {
                manifest: manifest.clone(),
                features: String::new(),
            },
            RunRequest {
                manifest,
                features: "macros".to_owned(),
            },
        ],
        "the specification requires the renamed fixture to be run with default features and with \
         macros, not merely compiled"
    );
}

#[test]
fn embedding_gate_reports_the_feature_set_that_failed() {
    let runner = RunRecorder::with_outcomes(vec![
        Ok(ProcessOutcome::Success),
        Ok(ProcessOutcome::Failed(
            "checks failed: absent_source_escalates".to_owned(),
        )),
    ]);

    let error = check_embedding(&workspace_root(), &runner).expect_err("second run fails");

    let report = error.to_string();
    assert!(
        report.contains("features `macros`"),
        "the report must name the failing feature set, got: {report}"
    );
    assert!(
        report.contains("absent_source_escalates"),
        "the report must quote the fixture's own output so the failing check is identifiable, got: \
         {report}"
    );
}

#[test]
fn embedding_gate_stops_at_the_first_failing_feature_set() {
    let runner = RunRecorder::with_outcomes(vec![Ok(ProcessOutcome::Failed("boom".to_owned()))]);

    check_embedding(&workspace_root(), &runner).expect_err("first run fails");

    assert_eq!(
        runner.requests().len(),
        1,
        "a failing run must not also report the later feature set, which would imply the first \
         passed"
    );
}

#[test]
fn embedding_gate_reports_unformatted_fixture_workspace() {
    let runner = RunRecorder::unformatted();

    let error =
        check_embedding(&workspace_root(), &runner).expect_err("unformatted fixture must fail");

    let report = error.to_string();
    assert!(
        report.contains("not rustfmt-clean"),
        "the report must say the fixture drifted out of the repository's formatting rules, got: \
         {report}"
    );
    assert!(
        report.contains("its own workspace"),
        "the report must explain why the repository's own fmt gate cannot catch it, got: {report}"
    );
    assert!(
        runner.requests().is_empty(),
        "formatting is checked before the fixture runs, so a misformatted fixture never spends a \
         build"
    );
}
