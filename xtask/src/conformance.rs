//! Normative specification conformance checks.

use std::{collections::BTreeSet, path::Path};

use crate::{ProcessOutcome, ProcessRunner, XtaskError};

/// One ordered specification conformance failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceFailure {
    /// Check category.
    pub check: &'static str,
    /// Failure detail.
    pub message: String,
}

/// Aggregate specification conformance report.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConformanceReport {
    /// Every failure in deterministic check order.
    pub failures: Vec<ConformanceFailure>,
}

/// Validates specification text while collecting independent failures.
///
/// # Errors
///
/// Returns immediately when scratch placement or process startup is unsafe.
pub fn validate_specification(
    specification: &str,
    scratch: &Path,
    runner: &impl ProcessRunner,
) -> Result<ConformanceReport, XtaskError> {
    if !scratch.starts_with(Path::new(".ctx/_WORKING_DIR/xtask-conformance")) {
        return Err(XtaskError::Conformance(
            "conformance scratch must stay under .ctx/_WORKING_DIR/xtask-conformance".into(),
        ));
    }
    let mut failures = Vec::new();
    if REQUIRED_HEADINGS
        .iter()
        .any(|heading| !specification.contains(heading))
    {
        push(
            &mut failures,
            "headings",
            "required Markdown headings are missing",
        );
    }
    if specification.contains("](#missing)") {
        push(&mut failures, "links", "Markdown link target is missing");
    }
    if [
        "TODO",
        "TBD",
        "todo!()",
        "unimplemented!()",
        "where feasible",
    ]
    .iter()
    .any(|marker| specification.contains(marker))
    {
        push(
            &mut failures,
            "incomplete",
            "forbidden incomplete marker found",
        );
    }

    let blocks = fenced_blocks(specification);
    if blocks
        .iter()
        .filter(|(language, _)| *language == "yaml")
        .any(|(_, body)| serde_yaml_ng::from_str::<serde_yaml_ng::Value>(body).is_err())
    {
        push(&mut failures, "yaml", "invalid YAML fenced block");
    }
    if blocks
        .iter()
        .filter(|(language, _)| *language == "json")
        .any(|(_, body)| serde_json::from_str::<serde_json::Value>(body).is_err())
    {
        push(&mut failures, "json", "invalid JSON fenced block");
    }
    let rust_blocks = blocks
        .iter()
        .filter(|(language, _)| *language == "rust")
        .collect::<Vec<_>>();
    let mut rustfmt_failed = false;
    for (index, (_, body)) in rust_blocks.into_iter().enumerate() {
        std::fs::create_dir_all(scratch)
            .map_err(|error| XtaskError::Conformance(error.to_string()))?;
        let path = scratch.join(format!("snippet-{index}.rs"));
        std::fs::write(&path, body).map_err(|error| XtaskError::Conformance(error.to_string()))?;
        if !runner
            .rustfmt_check(&path)
            .map_err(XtaskError::Conformance)?
        {
            rustfmt_failed = true;
        }
    }
    if rustfmt_failed {
        push(
            &mut failures,
            "rustfmt",
            "Rust fenced block is not formatted",
        );
    }
    if check_registry(specification).is_err() {
        push(
            &mut failures,
            "registry",
            "diagnostic registry is incomplete or duplicated",
        );
    }
    if SCHEMA_TAGS.iter().any(|tag| !specification.contains(tag)) {
        push(&mut failures, "schemas", "seven schema tags are incomplete");
    }
    if check_hash_vectors(specification).is_err() {
        push(
            &mut failures,
            "hash-vectors",
            "four fixed BLAKE3 vectors changed",
        );
    }
    Ok(ConformanceReport { failures })
}

fn push(failures: &mut Vec<ConformanceFailure>, check: &'static str, message: &str) {
    failures.push(ConformanceFailure {
        check,
        message: message.to_owned(),
    });
}

fn fenced_blocks(specification: &str) -> Vec<(&str, String)> {
    let mut language = None;
    let mut body = String::new();
    let mut blocks = Vec::new();
    for line in specification.lines() {
        if let Some(info) = line.strip_prefix("```") {
            if let Some(current) = language.take() {
                blocks.push((current, std::mem::take(&mut body)));
            } else {
                language = Some(info.trim());
            }
        } else if language.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    blocks
}

const REQUIRED_HEADINGS: &[&str] = &[
    "## Workspace and dependency architecture",
    "## Core contracts",
    "## Authored language",
    "## Evaluation semantics",
    "## Canonical serialization and hashing",
    "## Diagnostics",
    "## Analysis",
    "## Scenarios",
    "## CLI",
];

const SCHEMA_TAGS: &[&str] = &[
    "rulery.compiled-package/v1",
    "rulery.decision-trace/v1",
    "rulery.diagnostic-report/v1",
    "rulery.rulebook-lock/v1",
    "rulery.scenario-result/v1",
    "rulery.analysis-report/v1",
    "rulery.decision-table/v1",
];

const EXPECTED_VECTORS: &[(&str, &str)] = &[
    (
        "000000000000001a72756c6572792e636f6d70696c65642d7061636b6167652e763100000000000000027b7d",
        "1c6402278430173b5964f65d31c1ec06a9765c13c6e34bad762fcdee8ffbd6c8",
    ),
    (
        "000000000000001472756c6572792e636173652d66616374732e763100000000000000027b7d",
        "fa785b19b5601f57afd5548934d2c1e4b20bc1a8ab25b89e6bece83d041d31aa",
    ),
    (
        "000000000000001872756c6572792e6465636973696f6e2d74726163652e763100000000000000027b7d",
        "4773d0e94079df21090915deecb6b2f41327122e6b479afb63d5d082f174aced",
    ),
    (
        "000000000000001472756c6572792e6576616c756174696f6e2e7631000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000008636865636b6f75740000000000000020111111111111111111111111111111111111111111111111111111111111111100000000000000100000000000000000000000000000000000000000000000277b22696d706c656d656e746174696f6e223a2274657374222c2276657273696f6e223a2231227d",
        "4c11eb70b89e8543605015c325e309fde68d9007c2086b97bad892693cae9279",
    ),
];

pub(crate) fn check(root: &Path) -> Result<(), XtaskError> {
    let path = root.join("docs/specification.md");
    let specification = std::fs::read_to_string(&path)
        .map_err(|error| XtaskError::Conformance(format!("{}: {error}", path.display())))?;

    check_requirements(root, &specification)?;
    check_macro_hygiene(root, &crate::HostProcessRunner)?;

    let report = validate_specification(
        &specification,
        Path::new(".ctx/_WORKING_DIR/xtask-conformance"),
        &crate::HostProcessRunner,
    )?;
    if report.failures.is_empty() {
        Ok(())
    } else {
        Err(XtaskError::Conformance(
            report
                .failures
                .iter()
                .map(|failure| format!("{}: {}", failure.check, failure.message))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }
}

/// Manifest of the downstream crate that renames its `rulery` dependency.
const MACRO_HYGIENE_MANIFEST: &str = "examples/macro-hygiene/Cargo.toml";

/// Feature configurations the hygiene fixture must compile under, as the specification's
/// `Required conformance gates` bullet requires: `renamed-dependency macro hygiene tests with
/// default features and `macros` enabled`.
const MACRO_HYGIENE_FEATURE_SETS: &[&str] = &["", "macros"];

/// Compiles the renamed-dependency macro hygiene fixture under every required feature set.
///
/// The facade's declarative macros expand through `$crate::__private`, and `rulery-macros` locates
/// the facade by name, so both only stay hygienic when a downstream crate renames `rulery`. No
/// in-tree test can observe that: every one of them sees the dependency under its real name. This
/// check is the only place the claim is actually proven, so a fixture that stops compiling, or a
/// dependency rename that is quietly reverted, fails the gate rather than rotting unnoticed.
///
/// # Errors
///
/// Returns [`XtaskError::Conformance`] when the fixture fails to compile under a required feature
/// set, naming the feature set and quoting cargo's own diagnostics.
pub fn check_macro_hygiene(root: &Path, runner: &impl ProcessRunner) -> Result<(), XtaskError> {
    let manifest = root.join(MACRO_HYGIENE_MANIFEST);
    for features in MACRO_HYGIENE_FEATURE_SETS {
        let label = if features.is_empty() {
            "default features".to_owned()
        } else {
            format!("features `{features}`")
        };
        match runner
            .cargo_check(&manifest, features)
            .map_err(XtaskError::Conformance)?
        {
            ProcessOutcome::Success => {}
            ProcessOutcome::Failed(output) => {
                return Err(XtaskError::Conformance(format!(
                    "{MACRO_HYGIENE_MANIFEST} does not compile with {label}:\n{output}"
                )));
            }
        }
    }
    Ok(())
}

/// Requirement identifiers the v0.1 specification must declare.
const REQUIREMENT_IDS: &[&str] = &[
    "V01", "V02", "V03", "V04", "V05", "V06", "V07", "V08", "V09", "V10", "V11", "V12",
];

/// Static-problem identifiers nested under requirement `V10`.
const STATIC_CHECK_IDS: &[&str] = &[
    "S01", "S02", "S03", "S04", "S05", "S06", "S07", "S08", "S09",
];

/// Heading that introduces the normative requirement traceability tables.
const TRACEABILITY_HEADING: &str = "### Versioned requirement traceability";

/// One row of the specification's requirement traceability tables.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RequirementRow {
    /// Declared requirement or static-check identifier.
    id: String,
    /// Test named as the verification, empty when the row defers to another table.
    verification: String,
    /// Declared satisfaction status.
    status: String,
}

/// Rejects a specification whose declared requirements are incomplete, unverified, or unsatisfied.
///
/// A requirement is verified only when the test it names exists in the workspace, and satisfied
/// only when the specification says so. Naming a test that does not exist, omitting a
/// requirement, or leaving a requirement unsatisfied all fail the gate.
///
/// # Errors
///
/// Returns [`XtaskError::Conformance`] listing every missing identifier, missing test, and
/// unsatisfied requirement.
pub fn check_requirements(root: &Path, specification: &str) -> Result<(), XtaskError> {
    let rows = requirement_rows(specification);
    let mut failures = Vec::new();

    let declared: BTreeSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    for id in REQUIREMENT_IDS.iter().chain(STATIC_CHECK_IDS.iter()) {
        if !declared.contains(id) {
            failures.push(format!("{id} is not declared under {TRACEABILITY_HEADING}"));
        }
    }

    let tests = collect_test_names(root);
    for row in &rows {
        if !row.verification.is_empty() && !tests.contains(&row.verification) {
            failures.push(format!(
                "{} names test {}, which does not exist in the workspace",
                row.id, row.verification
            ));
        }
    }

    for row in &rows {
        if row.status != "satisfied" {
            failures.push(format!("{} is {}, not satisfied", row.id, row.status));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(XtaskError::Conformance(failures.join("\n")))
    }
}

/// Parses the traceability rows declared under [`TRACEABILITY_HEADING`].
fn requirement_rows(specification: &str) -> Vec<RequirementRow> {
    let Some(section) = specification.split(TRACEABILITY_HEADING).nth(1) else {
        return Vec::new();
    };
    let body = section.split("\n## ").next().unwrap_or(section);

    let mut rows = Vec::new();
    for line in body.lines() {
        let cells: Vec<&str> = line
            .split('|')
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
            .collect();
        if cells.len() != 4 {
            continue;
        }
        if !REQUIREMENT_IDS.contains(&cells[0]) && !STATIC_CHECK_IDS.contains(&cells[0]) {
            continue;
        }
        rows.push(RequirementRow {
            id: cells[0].to_owned(),
            verification: cells[2]
                .strip_prefix('`')
                .and_then(|cell| cell.strip_suffix('`'))
                .unwrap_or_default()
                .to_owned(),
            status: cells[3].to_owned(),
        });
    }
    rows
}

/// Collects every function name declared in a Rust source file in the workspace.
fn collect_test_names(root: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("target") {
                    pending.push(path);
                }
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in source.lines() {
                let Some(signature) = line.trim().strip_prefix("fn ") else {
                    continue;
                };
                let name = signature.split('(').next().unwrap_or_default().trim();
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    names.insert(name.to_owned());
                }
            }
        }
    }
    names
}

fn check_registry(specification: &str) -> Result<(), XtaskError> {
    let registry = specification
        .split("### Complete v0.1 known-code registry")
        .nth(1)
        .and_then(|text| text.split("\n## Analysis").next())
        .ok_or_else(|| XtaskError::Conformance("diagnostic registry section is missing".into()))?;
    let rows: Vec<_> = registry
        .lines()
        .filter_map(|line| {
            let start = line.find("`RUL")? + 1;
            let code = line.get(start..start + 6)?;
            (code.starts_with("RUL") && code[3..].bytes().all(|byte| byte.is_ascii_digit()))
                .then_some(code)
        })
        .collect();
    let codes: BTreeSet<_> = rows.iter().copied().collect();
    require(
        rows.len() == 40 && codes.len() == rows.len(),
        format!(
            "expected 40 unique diagnostic codes, found {} rows and {} unique codes",
            rows.len(),
            codes.len()
        ),
    )
}

fn check_hash_vectors(specification: &str) -> Result<(), XtaskError> {
    let mut pending_hex: Option<&str> = None;
    let mut vectors = Vec::new();

    for line in specification.lines() {
        if let Some(value) = line.strip_prefix("hex: ") {
            pending_hex = Some(value.trim());
        } else if let Some(expected) = line.strip_prefix("hash: blake3:") {
            let encoded = pending_hex.take().ok_or_else(|| {
                XtaskError::Conformance("hash vector is missing framed hex".into())
            })?;
            let bytes =
                hex::decode(encoded).map_err(|error| XtaskError::Conformance(error.to_string()))?;
            let actual = blake3::hash(&bytes).to_hex().to_string();
            require(actual == expected.trim(), "BLAKE3 vector does not match")?;
            vectors.push((encoded, expected.trim()));
        }
    }

    require(vectors == EXPECTED_VECTORS, "fixed BLAKE3 vectors changed")
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), XtaskError> {
    if condition {
        Ok(())
    } else {
        Err(XtaskError::Conformance(message.into()))
    }
}
