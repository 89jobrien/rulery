//! Normative specification conformance checks.

use std::{collections::BTreeSet, path::Path};

use crate::{ProcessRunner, XtaskError};

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
