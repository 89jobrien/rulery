//! Consumer embedding contract checks.
//!
//! The rest of the gate suite proves Rulery against its own specification: the normative
//! tool-library fixture, the declared wire envelopes, the compiled truth tables, and the CLI process
//! boundary. Those are all in-tree, and an in-tree test can reach any module it likes.
//!
//! None of them prove the thing a crates.io consumer actually depends on: that the *public facade*
//! is sufficient on its own. A refactor could move `assemble` behind an internal path, reshape
//! `CaseFacts`, or change which type carries the compiled package, and every in-tree gate would stay
//! green while the crate stopped being embeddable.
//!
//! This module closes that gap by running an out-of-tree fixture that depends on the facade under a
//! renamed name. The fixture is executed, not merely compiled: a consumer contract that only had to
//! resolve would miss a decision that evaluates to the wrong answer.

use std::path::Path;

use crate::{ProcessOutcome, ProcessRunner, XtaskError};

/// Manifest of the out-of-tree consumer fixture, relative to the workspace root.
const EMBEDDING_MANIFEST: &str = "examples/embedding-fixture/Cargo.toml";

/// Feature configurations the embedding fixture must satisfy.
///
/// Mirrors the specification's requirement that the renamed-dependency surface be exercised with
/// default features and with `macros` enabled, extended from compiling to running because the
/// procedural derive changes how facts are constructed, which is the part most likely to differ from
/// the hand-built adapter path.
const EMBEDDING_FEATURE_SETS: &[&str] = &["", "macros"];

/// Builds and runs the embedding fixture under every required feature set.
///
/// # Errors
///
/// Returns [`XtaskError::Embedding`] when the fixture fails to build or run under a required
/// feature set, naming the feature set and quoting cargo's own output.
pub fn check_embedding(root: &Path, runner: &impl ProcessRunner) -> Result<(), XtaskError> {
    let manifest = root.join(EMBEDDING_MANIFEST);
    if !runner
        .cargo_fmt_check(&manifest)
        .map_err(XtaskError::Embedding)?
    {
        return Err(XtaskError::Embedding(format!(
            "{EMBEDDING_MANIFEST} is not rustfmt-clean. The fixture is its own workspace, so the \
             repository's `cargo fmt --all` gate cannot see it."
        )));
    }
    for features in EMBEDDING_FEATURE_SETS {
        let label = if features.is_empty() {
            "default features".to_owned()
        } else {
            format!("features `{features}`")
        };
        match runner
            .cargo_run(&manifest, features)
            .map_err(XtaskError::Embedding)?
        {
            ProcessOutcome::Success => {}
            ProcessOutcome::Failed(output) => {
                return Err(XtaskError::Embedding(format!(
                    "{EMBEDDING_MANIFEST} does not satisfy the embedding contract with {label}:\n\
                     {output}"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn run(root: &Path) -> Result<(), XtaskError> {
    check_embedding(root, &crate::HostProcessRunner)
}
