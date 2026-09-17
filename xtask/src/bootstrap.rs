//! Workspace bootstrap drift detection.

use std::path::Path;

use crate::{XtaskError, model};

pub(crate) fn check(root: &Path) -> Result<(), XtaskError> {
    let root_manifest = root.join("Cargo.toml");
    let root_contents = read(&root_manifest)?;
    require(
        root_contents.contains("members = [\"crates/*\", \"xtask\"]"),
        "root workspace members differ from the declarative model",
    )?;

    for (directory, package) in model::SCAFFOLD_CRATES {
        let manifest = root.join(directory).join("Cargo.toml");
        let contents = read(&manifest)?;
        require(
            contents.contains(&format!("name = \"{package}\"")),
            format!("{} does not declare `{package}`", manifest.display()),
        )?;
        let source = if *directory == "crates/cli" || *directory == "xtask" {
            root.join(directory).join("src/main.rs")
        } else {
            root.join(directory).join("src/lib.rs")
        };
        require(
            source.is_file(),
            format!("missing scaffold entry point {}", source.display()),
        )?;
    }

    Ok(())
}

fn read(path: &Path) -> Result<String, XtaskError> {
    std::fs::read_to_string(path)
        .map_err(|error| XtaskError::Bootstrap(format!("{}: {error}", path.display())))
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), XtaskError> {
    if condition {
        Ok(())
    } else {
        Err(XtaskError::Bootstrap(message.into()))
    }
}
