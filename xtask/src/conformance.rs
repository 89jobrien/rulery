//! Normative specification conformance checks.

use std::{collections::BTreeSet, path::Path};

use crate::XtaskError;

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

    for heading in REQUIRED_HEADINGS {
        require(
            specification.contains(heading),
            format!("missing heading `{heading}`"),
        )?;
    }
    for marker in [
        "TODO",
        "TBD",
        "todo!()",
        "unimplemented!()",
        "where feasible",
    ] {
        require(
            !specification.contains(marker),
            format!("forbidden incomplete marker `{marker}`"),
        )?;
    }
    for schema in SCHEMA_TAGS {
        require(
            specification.contains(schema),
            format!("missing schema `{schema}`"),
        )?;
    }

    check_fenced_data(&specification)?;
    check_registry(&specification)?;
    check_hash_vectors(&specification)?;
    Ok(())
}

fn check_fenced_data(specification: &str) -> Result<(), XtaskError> {
    let mut language: Option<&str> = None;
    let mut body = String::new();

    for line in specification.lines() {
        if let Some(info) = line.strip_prefix("```") {
            if let Some(current) = language.take() {
                match current {
                    "yaml" => {
                        serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&body)
                            .map_err(|error| XtaskError::Conformance(error.to_string()))?;
                    }
                    "json" => {
                        serde_json::from_str::<serde_json::Value>(&body)
                            .map_err(|error| XtaskError::Conformance(error.to_string()))?;
                    }
                    _ => {}
                }
                body.clear();
            } else {
                language = Some(info.trim());
            }
        } else if language.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }

    require(language.is_none(), "unterminated fenced code block")
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
