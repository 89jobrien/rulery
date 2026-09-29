//! Process-level CLI contract tests.
//!
//! These tests spawn the real `rulery` binary and pin the contracts that exist only at the process
//! boundary: the exit codes the specification's exit-condition matrix names, single-artifact stdout
//! cardinality, stderr discipline in machine modes, and the side-effect rules the specification
//! states as requirements. Everything asserted here is observed from a finished process rather than
//! from a library return value, so a regression in argument parsing, stream routing, or process
//! exit fails here even when the library API is unchanged.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The built `rulery` binary, resolved by cargo for this integration test.
const BINARY: &str = env!("CARGO_BIN_EXE_rulery");

/// The normative tool-library fixture, resolved from the workspace root.
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/tool-library");

/// The instant at which the tool-library conformance example is specified to be evaluated.
const INSTANT: &str = "2026-09-16T16:00:00.000000000Z";

/// The wire form of [`INSTANT`], which `UtcInstant` serializes as canonical signed Unix
/// nanoseconds rather than the RFC 3339 text the flag accepts.
const INSTANT_NANOSECONDS: &str = "1789574400000000000";

/// The multi-package import fixture, resolved from the workspace root. Its root package imports a
/// second local package, so resolving it exercises the store, parser, and assembler together.
const IMPORT_GRAPH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/import-graph");

/// The single decision declared by the fixture manifest.
const DECISION: &str = "checkout";

/// Environment variables a CI runner plausibly sets, none of which may influence lock mode.
const CI_ENVIRONMENT: [(&str, &str); 4] = [
    ("CI", "true"),
    ("CONTINUOUS_INTEGRATION", "true"),
    ("RULERY_FROZEN", "1"),
    ("RULERY_LOCK_MODE", "frozen"),
];

/// One completed process invocation reduced to the three things the contract names.
struct Invocation {
    /// Process exit code.
    code: i32,
    /// Captured standard output.
    stdout: String,
    /// Captured standard error.
    stderr: String,
}

/// Runs the binary with a cleared environment, so no ambient value can leak into the result.
fn run(args: &[&str]) -> Invocation {
    invoke(args, &[])
}

/// Runs the binary with a cleared environment plus the supplied variables.
fn invoke(args: &[&str], environment: &[(&str, &str)]) -> Invocation {
    let mut command = Command::new(BINARY);
    command.args(args).env_clear();
    for (key, value) in environment {
        command.env(key, value);
    }
    let Output {
        status,
        stdout,
        stderr,
    } = command.output().expect("binary executes");
    Invocation {
        code: status.code().expect("process exits normally"),
        stdout: String::from_utf8(stdout).expect("utf-8 stdout"),
        stderr: String::from_utf8(stderr).expect("utf-8 stderr"),
    }
}

/// Returns the path of the fixture's case file.
fn fixture_facts() -> String {
    path(
        &Path::new(FIXTURE)
            .join("cases")
            .join("expired-training.yaml"),
    )
}

/// Renders one path as a string the binary can receive as an argument.
fn path(value: &Path) -> String {
    value.to_string_lossy().into_owned()
}

/// Returns a fresh empty scratch directory for one test.
fn scratch(label: &str) -> PathBuf {
    let scratch = std::env::temp_dir().join(format!(
        "rulery-process-contract-{}-{label}",
        std::process::id()
    ));
    fs::remove_dir_all(&scratch).ok();
    fs::create_dir_all(&scratch).expect("scratch directory");
    scratch
}

/// Returns a fresh copy of the fixture inside a per-test scratch directory.
fn package(label: &str) -> PathBuf {
    let scratch = scratch(label);
    let copy = scratch.join("tool-library");
    copy_tree(Path::new(FIXTURE), &copy);
    copy
}

/// Removes the scratch directory that holds one copied package.
fn discard(package: &Path) {
    if let Some(scratch) = package.parent() {
        fs::remove_dir_all(scratch).ok();
    }
}

/// Recursively copies one directory tree, creating the destination.
fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory");
    for entry in fs::read_dir(source).expect("readable source") {
        let entry = entry.expect("readable entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copied file");
        }
    }
}

/// Returns every file under `root` paired with its bytes, sorted by relative path.
///
/// Comparing two snapshots is how the side-effect rules are checked: a command that writes an
/// artifact file changes this list, and one that only reads does not.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries = Vec::new();
    collect(root, root, &mut entries);
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

/// Recursively appends one file's relative path and bytes to `entries`.
fn collect(root: &Path, directory: &Path, entries: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(directory).expect("readable directory") {
        let entry = entry.expect("readable entry");
        let target = entry.path();
        if entry.file_type().expect("file type").is_dir() {
            collect(root, &target, entries);
        } else {
            let relative = target
                .strip_prefix(root)
                .expect("descendant of root")
                .to_string_lossy()
                .into_owned();
            entries.push((relative, fs::read(&target).expect("readable file")));
        }
    }
}

/// Parses one command's stdout as exactly one JSON value.
fn one_json_value(label: &str, stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout)
        .unwrap_or_else(|error| panic!("{label}: stdout is not one JSON value: {error}"))
}

/// Replaces every `0` in the copied lock so that it no longer matches its package.
fn stale_lock(package: &Path) {
    let lock = package.join("rulery.lock");
    let intact = fs::read_to_string(&lock).expect("readable lock");
    assert_ne!(
        intact,
        intact.replace('0', "1"),
        "lock fixture contains no digit to perturb"
    );
    fs::write(&lock, intact.replace('0', "1")).expect("writable lock");
}

#[test]
fn exit_codes_match_the_specification_matrix() {
    let facts = fixture_facts();
    for (label, args, expected) in [
        ("check success", vec!["check", FIXTURE, "--frozen"], 0),
        (
            "check success as json",
            vec!["check", FIXTURE, "--frozen", "--format", "json"],
            0,
        ),
        (
            "check success as sarif",
            vec!["check", FIXTURE, "--frozen", "--format", "sarif"],
            0,
        ),
        ("scenarios pass", vec!["test", FIXTURE, "--frozen"], 0),
        (
            "analysis incomplete",
            vec!["analyze", FIXTURE, "--frozen"],
            6,
        ),
        (
            "analysis incomplete as json",
            vec!["analyze", FIXTURE, "--frozen", "--format", "json"],
            6,
        ),
        (
            "render markdown",
            vec!["render", FIXTURE, "--frozen", "--format", "markdown"],
            0,
        ),
        (
            "render decision table",
            vec![
                "render",
                FIXTURE,
                "--frozen",
                "--format",
                "decision-table",
                "--decision",
                DECISION,
            ],
            0,
        ),
        (
            "explain one decision",
            vec![
                "explain",
                FIXTURE,
                "--frozen",
                "--decision",
                DECISION,
                "--facts",
                &facts,
                "--at",
                INSTANT,
            ],
            0,
        ),
        (
            "requested format is invalid",
            vec!["check", FIXTURE, "--format", "markdown"],
            2,
        ),
        (
            "unknown decision",
            vec![
                "render",
                FIXTURE,
                "--frozen",
                "--format",
                "decision-table",
                "--decision",
                "absent",
            ],
            2,
        ),
        (
            "decision table without a decision",
            vec!["render", FIXTURE, "--frozen", "--format", "decision-table"],
            2,
        ),
        (
            "required path is missing",
            vec!["check", "no/such/package"],
            3,
        ),
    ] {
        let observed = run(&args);
        assert_eq!(observed.code, expected, "{label}: exit code");
    }
}

#[test]
fn invocation_failures_report_on_stderr_with_an_empty_stdout() {
    for (label, args) in [
        ("no command supplied", vec![]),
        ("unrecognized subcommand", vec!["frobnicate"]),
        ("required argument absent", vec!["explain", FIXTURE]),
        (
            "argument value is malformed",
            vec![
                "explain",
                FIXTURE,
                "--decision",
                "INVALID",
                "--facts",
                "facts.json",
            ],
        ),
    ] {
        let observed = run(&args);
        assert_eq!(observed.code, 2, "{label}: exit code");
        assert!(observed.stdout.is_empty(), "{label}: wrote an artifact");
        assert!(
            !observed.stderr.trim().is_empty(),
            "{label}: reported nothing on stderr"
        );
    }
}

/// Asserts that every command whose JSON output is a versioned envelope emits exactly one.
///
/// `test` is the specification's documented exception and is covered by
/// [`test_json_emits_the_documented_scenario_result_array`].
#[test]
fn machine_formats_emit_one_envelope_and_no_stderr() {
    let facts = fixture_facts();
    for (label, args) in [
        (
            "check",
            vec!["check", FIXTURE, "--frozen", "--format", "json"].as_slice(),
        ),
        (
            "analyze",
            vec!["analyze", FIXTURE, "--frozen", "--format", "json"].as_slice(),
        ),
        (
            "explain",
            vec![
                "explain",
                FIXTURE,
                "--frozen",
                "--decision",
                DECISION,
                "--facts",
                &facts,
                "--at",
                INSTANT,
                "--format",
                "json",
            ]
            .as_slice(),
        ),
        (
            "render",
            vec!["render", FIXTURE, "--frozen", "--format", "json"].as_slice(),
        ),
    ] {
        let observed = run(args);
        assert!(
            observed.stderr.is_empty(),
            "{label}: machine mode wrote to stderr"
        );
        let artifact = one_json_value(label, &observed.stdout);
        assert!(
            artifact.get("schema").is_some(),
            "{label}: artifact is not a versioned envelope"
        );
        assert!(
            artifact.get("payload").is_some(),
            "{label}: envelope carries no payload"
        );
    }
}

#[test]
fn test_json_emits_the_documented_scenario_result_array() {
    let observed = run(&["test", FIXTURE, "--frozen", "--format", "json"]);
    assert_eq!(observed.code, 0);
    assert!(observed.stderr.is_empty(), "machine mode wrote to stderr");
    let serde_json::Value::Array(results) = one_json_value("test", &observed.stdout) else {
        panic!("test must emit an array of scenario results");
    };
    assert!(!results.is_empty(), "scenario result array is empty");
    for result in &results {
        assert_eq!(
            result.get("schema").and_then(serde_json::Value::as_str),
            Some("rulery.scenario-result/v1"),
            "array element is not a scenario-result envelope"
        );
    }
}

#[test]
fn check_frozen_accepts_the_import_graph_fixture() {
    let scratch = scratch("import-graph");
    let copy = scratch.join("import-graph");
    copy_tree(Path::new(IMPORT_GRAPH), &copy);
    let copy_path = path(&copy);

    let checked = run(&["check", &copy_path, "--frozen", "--format", "json"]);
    assert_eq!(
        checked.code, 0,
        "check --frozen rejected the frozen closure"
    );
    assert!(checked.stderr.is_empty(), "machine mode wrote to stderr");
    one_json_value("check --frozen", &checked.stdout);

    let tested = run(&["test", &copy_path, "--frozen", "--format", "json"]);
    assert_eq!(tested.code, 0, "test --frozen rejected the frozen closure");
    assert!(tested.stderr.is_empty(), "machine mode wrote to stderr");
    one_json_value("test --frozen", &tested.stdout);

    // Frozen mode must actually reject drift, or the two assertions above prove nothing. The
    // drift has to stay valid YAML: appending a byte at end of file would make it a parse
    // error, which also exits non-zero but says nothing about the lock. Changing an authored
    // description is a well-formed edit that moves the imported package's content hash.
    let shared = copy.join("imports").join("shared").join("actions.yaml");
    let original = fs::read_to_string(&shared).expect("readable fixture");
    let drifted = original.replace(
        "Records an approved checkout obligation.",
        "Records an approved checkout obligation. ",
    );
    assert_ne!(
        drifted, original,
        "the fixture wording changed; update this test"
    );
    fs::write(&shared, drifted).expect("writeable fixture");

    let rejected = run(&["check", &copy_path, "--frozen", "--format", "json"]);
    assert_ne!(
        rejected.code, 0,
        "frozen mode accepted a closure whose bytes no longer match the lock"
    );
    // In machine mode the rejection is a diagnostic envelope on stdout, and stderr stays empty.
    let envelope = one_json_value("drifted check", &rejected.stdout);
    assert!(
        envelope.to_string().contains("RUL501"),
        "expected a lock-disagreement diagnostic, got: {}",
        rejected.stdout
    );
    discard(&copy);
}

#[test]
fn machine_formats_emit_one_envelope_on_pre_artifact_failure() {
    let observed = run(&["analyze", "/nonexistent/rulery-fixture", "--format", "json"]);
    assert_eq!(
        observed.code, 3,
        "expected the IoFailure row of the exit matrix"
    );
    one_json_value("analyze missing path", &observed.stdout);
}

#[test]
fn analyze_at_is_recorded_in_the_report() {
    let copy = package("analyze-at");
    let baseline = run(&["analyze", &path(&copy), "--frozen", "--format", "json"]);
    let observed = run(&[
        "analyze",
        &path(&copy),
        "--frozen",
        "--at",
        INSTANT,
        "--format",
        "json",
    ]);
    discard(&copy);

    assert_eq!(
        observed.code, baseline.code,
        "--at changed the exit-condition row"
    );
    assert!(observed.stderr.is_empty(), "machine mode wrote to stderr");
    let artifact = one_json_value("analyze --at", &observed.stdout);
    let Some(witnesses) = artifact
        .get("payload")
        .and_then(|payload| payload.get("witnesses"))
        .and_then(serde_json::Value::as_array)
    else {
        panic!("analysis envelope carries no witness array");
    };
    assert!(
        !witnesses.is_empty(),
        "analysis produced no witness to observe the instant on"
    );
    for witness in witnesses {
        assert_eq!(
            witness.get("at").and_then(serde_json::Value::as_str),
            Some(INSTANT_NANOSECONDS),
            "witness recorded another instant than --at requested"
        );
    }
}

#[test]
fn sarif_emits_one_log_and_is_not_a_rulery_envelope() {
    let observed = run(&["check", FIXTURE, "--frozen", "--format", "sarif"]);
    assert_eq!(observed.code, 0);
    assert!(observed.stderr.is_empty());
    let log = one_json_value("check --format sarif", &observed.stdout);
    assert_eq!(
        log.get("version").and_then(serde_json::Value::as_str),
        Some("2.1.0")
    );
    assert!(log.get("$schema").is_some(), "log carries no $schema");
    assert!(log.get("runs").is_some(), "log carries no runs");
    assert!(
        log.get("schema").is_none(),
        "a log must not be a Rulery envelope"
    );
}

#[test]
fn read_only_commands_write_no_artifact_file() {
    let facts = fixture_facts();
    let before = snapshot(Path::new(FIXTURE));
    for (label, args) in [
        ("check", vec!["check", FIXTURE, "--frozen"].as_slice()),
        ("test", vec!["test", FIXTURE, "--frozen"].as_slice()),
        ("analyze", vec!["analyze", FIXTURE, "--frozen"].as_slice()),
        (
            "explain",
            vec![
                "explain",
                FIXTURE,
                "--frozen",
                "--decision",
                DECISION,
                "--facts",
                &facts,
                "--at",
                INSTANT,
            ]
            .as_slice(),
        ),
        (
            "render",
            vec!["render", FIXTURE, "--frozen", "--format", "markdown"].as_slice(),
        ),
    ] {
        run(args);
        assert_eq!(
            snapshot(Path::new(FIXTURE)),
            before,
            "{label}: wrote a file into the package"
        );
    }
}

#[test]
fn explain_keeps_case_data_in_process_memory_only() {
    let copy = package("trace-persistence");
    let before = snapshot(&copy);
    let observed = run(&[
        "explain",
        &path(&copy),
        "--frozen",
        "--decision",
        DECISION,
        "--facts",
        &path(&copy.join("cases").join("expired-training.yaml")),
        "--at",
        INSTANT,
        "--format",
        "json",
    ]);
    assert_eq!(observed.code, 0);
    assert!(observed.stderr.is_empty());
    assert!(!observed.stdout.is_empty(), "explain produced no trace");
    assert_eq!(
        snapshot(&copy),
        before,
        "explain persisted a trace to the package"
    );
    discard(&copy);
}

#[test]
fn environment_variables_do_not_change_lock_mode() {
    let copy = package("lock-mode");
    stale_lock(&copy);
    let package_path = path(&copy);

    for (label, environment) in [
        ("without CI variables", &[][..]),
        ("with CI variables", &CI_ENVIRONMENT[..]),
    ] {
        let observed = invoke(&["check", &package_path], environment);
        assert_eq!(observed.code, 0, "{label}: update mode exit code");
        assert!(
            observed.stderr.is_empty(),
            "{label}: update mode reported a lock problem"
        );
    }

    for (label, environment) in [
        ("without CI variables", &[][..]),
        ("with CI variables", &CI_ENVIRONMENT[..]),
    ] {
        let observed = invoke(&["check", &package_path, "--frozen"], environment);
        assert_eq!(observed.code, 1, "{label}: frozen mode exit code");
        assert!(
            observed.stderr.contains("rulery.lock is out of date"),
            "{label}: unexpected stderr {}",
            observed.stderr
        );
    }

    discard(&copy);
}

/// Asserts that an intact lock succeeds identically under both modes and both environments.
///
/// A stale lock is the sharper probe for mode selection, and is used by
/// [`environment_variables_do_not_change_lock_mode`]; this one pins that the success row produces
/// the same artifact regardless of which mode or environment reached it.
#[test]
fn an_intact_lock_is_accepted_in_every_mode_and_environment() {
    let copy = package("lock-agreement");
    let package_path = path(&copy);
    let frozen = run(&["check", &package_path, "--frozen"]);
    let update = run(&["check", &package_path]);
    assert_eq!(frozen.code, 0, "--frozen rejected an intact lock");
    assert_eq!(update.code, 0, "update mode rejected an intact lock");
    assert_eq!(
        frozen.stdout, update.stdout,
        "modes disagreed on the artifact"
    );

    for (label, observed, reference) in [
        (
            "CI --frozen",
            invoke(&["check", &package_path, "--frozen"], &CI_ENVIRONMENT),
            &frozen,
        ),
        (
            "CI update",
            invoke(&["check", &package_path], &CI_ENVIRONMENT),
            &update,
        ),
    ] {
        assert_eq!(observed.code, reference.code, "{label}: exit code");
        assert_eq!(observed.stdout, reference.stdout, "{label}: artifact");
    }

    discard(&copy);
}

/// Asserts that `init` scaffolds the five required paths and that the result passes `check`.
///
/// The scaffold must already be in canonical form, so `fmt --check` on a fresh destination is
/// clean, and `check` must report no diagnostic.
#[test]
fn init_scaffolds_a_canonical_package_that_checks_clean() {
    let destination = scratch("init");
    let package = destination.join("community-tool-library");

    let observed = run(&["init", &path(&package)]);
    assert_eq!(observed.code, 0, "init: {}", observed.stderr);
    assert_eq!(
        observed.stdout.lines().count(),
        1,
        "init must write one human path line, got {}",
        observed.stdout
    );
    assert!(observed.stderr.is_empty(), "init wrote to stderr");

    for name in ["rulery.yaml", "vocabulary.yaml", "actions.yaml"] {
        assert!(package.join(name).is_file(), "init did not create {name}");
    }
    for name in ["rules", "scenarios"] {
        assert!(package.join(name).is_dir(), "init did not create {name}/");
    }
    assert!(
        !package.join("rulery.lock").exists(),
        "init must not write a lock"
    );

    let checked = run(&["check", &path(&package)]);
    assert_eq!(
        checked.code, 0,
        "scaffold does not check clean: {}",
        checked.stderr
    );
    assert!(
        checked.stdout.is_empty(),
        "check on a clean package reported findings"
    );

    let formatted = run(&["fmt", &path(&package), "--check"]);
    assert_eq!(
        formatted.code, 0,
        "a fresh scaffold is not canonical: {}",
        formatted.stdout
    );

    discard(&destination);
}

/// Asserts that `init` derives the package identity from the destination name and refuses a
/// non-empty destination.
#[test]
fn init_derives_identity_and_refuses_a_non_empty_destination() {
    let destination = scratch("init-identity");
    let package = destination.join("Tool Library");
    let observed = run(&["init", &path(&package)]);
    assert_eq!(observed.code, 0, "init: {}", observed.stderr);
    let manifest = std::fs::read_to_string(package.join("rulery.yaml")).expect("readable manifest");
    assert!(
        manifest.contains("id: tool-library"),
        "identity was not derived from the directory name: {manifest}"
    );
    assert!(
        manifest.contains("display_name: tool-library"),
        "display name was not derived: {manifest}"
    );

    let again = run(&["init", &path(&package)]);
    assert_eq!(again.code, 1, "init overwrote a non-empty destination");
    assert!(again.stdout.is_empty(), "init wrote an artifact on refusal");

    discard(&destination);
}

/// Asserts that `fmt FILE` reports canonical bytes on stdout and leaves the file untouched.
#[test]
fn fmt_of_one_file_reports_bytes_without_rewriting() {
    let package = package("fmt-one-file");
    let manifest = package.join("rulery.yaml");
    let original = std::fs::read_to_string(&manifest).expect("readable manifest");
    let perturbed = format!("# comment\n{original}");
    std::fs::write(&manifest, &perturbed).expect("writable manifest");

    let reported = run(&["fmt", &path(&manifest)]);
    assert_eq!(reported.code, 0, "fmt FILE: {}", reported.stderr);
    assert_eq!(
        std::fs::read_to_string(&manifest).expect("readable manifest"),
        perturbed,
        "fmt FILE rewrote the file it was asked to report"
    );
    assert!(
        !reported.stdout.contains('#'),
        "fmt FILE reported uncanonical bytes: {}",
        reported.stdout
    );
    assert!(
        reported.stdout.ends_with('\n'),
        "fmt FILE output has no trailing newline"
    );

    discard(&package);
}

/// Asserts that `fmt --check` reports every non-canonical file and that `fmt` fixes exactly those.
#[test]
fn fmt_check_reports_every_noncanonical_file() {
    let package = package("fmt");
    let manifest = package.join("rulery.yaml");
    // Re-indent and reorder the manifest so it is no longer canonical.
    let original = std::fs::read_to_string(&manifest).expect("readable manifest");
    std::fs::write(&manifest, format!("# a comment\n{original}")).expect("writable manifest");

    let checked = run(&["fmt", &path(&package), "--check"]);
    assert_eq!(checked.code, 1, "fmt --check missed a non-canonical file");
    assert!(
        checked.stdout.contains("rulery.yaml"),
        "no path reported: {}",
        checked.stdout
    );
    assert_eq!(
        std::fs::read_to_string(&manifest).expect("readable manifest"),
        format!("# a comment\n{original}"),
        "fmt --check wrote to the package"
    );

    let formatted = run(&["fmt", &path(&package)]);
    assert_eq!(formatted.code, 0, "fmt: {}", formatted.stderr);
    assert!(
        formatted.stdout.contains("rulery.yaml"),
        "fmt reported no rewritten path: {}",
        formatted.stdout
    );
    let canonical = std::fs::read_to_string(&manifest).expect("readable manifest");
    assert!(!canonical.contains('#'), "fmt kept a comment: {canonical}");

    let settled = run(&["fmt", &path(&package), "--check"]);
    assert_eq!(settled.code, 0, "fmt is not idempotent: {}", settled.stdout);
    assert!(
        settled.stdout.is_empty(),
        "fmt --check reported a settled package"
    );

    discard(&package);
}
