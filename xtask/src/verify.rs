//! Aggregate repository verification workflow.

use std::path::Path;

use xshell::{Shell, cmd};

use crate::{XtaskError, architecture, bootstrap, conformance};

pub(crate) fn run(root: &Path) -> Result<(), XtaskError> {
    bootstrap::check(root)?;
    conformance::check(root)?;
    architecture::check(root)?;

    let shell = Shell::new().map_err(|error| XtaskError::Command(error.to_string()))?;
    shell.change_dir(root);
    for command in [
        cmd!(shell, "cargo fmt --all --check"),
        cmd!(shell, "cargo clippy --workspace -- -D warnings"),
        cmd!(shell, "cargo nextest run --workspace"),
        cmd!(shell, "cargo doc --workspace --no-deps --lib"),
    ] {
        command
            .run()
            .map_err(|error| XtaskError::Command(error.to_string()))?;
    }
    Ok(())
}
