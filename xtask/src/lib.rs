//! Complex repository automation for Rulery.

#![forbid(unsafe_code)]

use clap::{Args, Parser, Subcommand};
use thiserror::Error;

mod architecture;
mod bootstrap;
mod conformance;
mod fs;
mod model;
mod process;
mod verify;

pub use architecture::{PackageSnapshot, validate_architecture};
pub use bootstrap::{Change, ChangeSet, ReconcileMode, plan_bootstrap, reconcile};
pub use conformance::{ConformanceFailure, ConformanceReport, validate_specification};
pub use fs::{HostFileSystem, WorkspaceFileSystem};
pub use model::{
    CrateSpec, DependencyRule, TargetKind, WorkspaceModel, model_dependencies, model_manifest,
    model_target, workspace_model,
};
pub use process::{HostProcessRunner, ProcessRunner};
pub use verify::{VerifyGate, VerifyRunner, verify_with};

/// Repository automation command line.
#[derive(Clone, Debug, Eq, PartialEq, Parser)]
#[command(name = "cargo xtask")]
pub struct Cli {
    /// Complex repository workflow to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Available repository workflows.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum Command {
    /// Reconcile the repository with the declarative workspace model.
    Bootstrap(BootstrapArgs),
    /// Validate the normative specification and embedded fixtures.
    Conformance,
    /// Validate workspace membership and dependency boundaries.
    Architecture,
    /// Run all repository checks in fail-fast order.
    Verify,
}

/// Options controlling workspace reconciliation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Args)]
pub struct BootstrapArgs {
    /// Report drift without changing files.
    #[arg(long, conflicts_with = "dry_run")]
    check: bool,
    /// Print planned changes without changing files.
    #[arg(long)]
    dry_run: bool,
}

/// Error returned by a repository workflow.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum XtaskError {
    /// A child repository command failed.
    #[error("repository command failed: {0}")]
    Command(String),
    /// Cargo metadata could not be loaded.
    #[error("failed to load Cargo metadata: {0}")]
    Metadata(String),
    /// A required package is missing from the workspace.
    #[error("required workspace package `{package}` is missing")]
    MissingMember {
        /// Missing Cargo package name.
        package: String,
    },
    /// A workspace package has a forbidden internal dependency.
    #[error("package `{package}` cannot depend on `{dependency}`")]
    ForbiddenDependency {
        /// Dependent Cargo package name.
        package: String,
        /// Forbidden dependency package name.
        dependency: String,
    },
    /// Internal dependency graph contains a cycle.
    #[error("workspace internal dependency cycle: {packages:?}")]
    DependencyCycle {
        /// Packages participating in the cycle.
        packages: Vec<String>,
    },
    /// The workspace contains a package absent from the model.
    #[error("unexpected workspace package `{package}`")]
    UnexpectedMember {
        /// Unexpected Cargo package name.
        package: String,
    },
    /// A package manifest is outside its modeled location.
    #[error("package `{package}` must use manifest `{expected}`")]
    WrongLocation {
        /// Cargo package name.
        package: String,
        /// Required relative manifest path.
        expected: String,
    },
    /// A package does not expose its modeled target type.
    #[error("package `{package}` must expose target kind `{expected}`")]
    WrongTarget {
        /// Cargo package name.
        package: String,
        /// Required Cargo target kind.
        expected: String,
    },
    /// The normative specification violates its conformance contract.
    #[error("specification conformance failed: {0}")]
    Conformance(String),
    /// The workspace scaffold differs from the declarative model.
    #[error("workspace bootstrap check failed: {0}")]
    Bootstrap(String),
}

/// Dispatches one repository workflow.
///
/// # Errors
///
/// Returns an error when the selected workflow detects drift or a command fails.
pub fn execute(cli: &Cli) -> Result<(), XtaskError> {
    match &cli.command {
        Command::Architecture => architecture::check(workspace_root()?),
        Command::Bootstrap(arguments) => {
            let mode = if arguments.check {
                ReconcileMode::Check
            } else if arguments.dry_run {
                ReconcileMode::DryRun
            } else {
                ReconcileMode::Apply
            };
            let changes = reconcile(&HostFileSystem, workspace_root()?, workspace_model(), mode)?;
            if mode == ReconcileMode::DryRun {
                for change in changes.changes {
                    println!("{change:?}");
                }
            } else if mode == ReconcileMode::Apply {
                println!(
                    "workspace bootstrap applied {} changes",
                    changes.changes.len()
                );
            }
            Ok(())
        }
        Command::Conformance => conformance::check(workspace_root()?),
        Command::Verify => verify::run(workspace_root()?),
    }
}

fn workspace_root() -> Result<&'static std::path::Path, XtaskError> {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| XtaskError::Conformance("xtask has no workspace parent".into()))
}
