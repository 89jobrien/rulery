//! Aggregate repository verification workflow.

use std::path::Path;

use xshell::{Shell, cmd};

use crate::{XtaskError, architecture, bootstrap, conformance};

/// Ordered verification gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyGate {
    /// Bootstrap drift check.
    BootstrapCheck,
    /// Specification conformance.
    Conformance,
    /// Metadata architecture validation.
    Architecture,
    /// rustfmt check.
    Format,
    /// Clippy warnings-denied check.
    Clippy,
    /// Workspace nextest suite.
    Nextest,
    /// Workspace doctest suite.
    Doctest,
    /// Workspace rustdoc build.
    Rustdoc,
}

/// Injected gate runner.
pub trait VerifyRunner {
    /// Runs one gate.
    ///
    /// # Errors
    ///
    /// Returns [`XtaskError`] when the selected gate fails or cannot start.
    fn run(&self, gate: VerifyGate) -> Result<(), XtaskError>;
}

/// Runs all verification gates in exact fail-fast order.
///
/// # Errors
///
/// Returns immediately after the first failed gate.
pub fn verify_with(runner: &impl VerifyRunner) -> Result<(), XtaskError> {
    for gate in [
        VerifyGate::BootstrapCheck,
        VerifyGate::Conformance,
        VerifyGate::Architecture,
        VerifyGate::Format,
        VerifyGate::Clippy,
        VerifyGate::Nextest,
        VerifyGate::Doctest,
        VerifyGate::Rustdoc,
    ] {
        runner.run(gate)?;
    }
    Ok(())
}

/// One external gate's process invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GateCommand {
    /// The program the gate executes.
    pub program: &'static str,
    /// The arguments, already split, in order.
    pub args: &'static [&'static str],
}

/// Returns the process one gate runs, or `None` when the gate runs in process.
///
/// The mapping is a single source of truth: the host runner executes what this returns, and the
/// integration test asserts the arguments, so a gate cannot quietly stop covering a target.
#[must_use]
pub const fn gate_command(gate: VerifyGate) -> Option<GateCommand> {
    let command = match gate {
        VerifyGate::Format => GateCommand {
            program: "cargo",
            args: &["fmt", "--all", "--check"],
        },
        VerifyGate::Clippy => GateCommand {
            program: "cargo",
            args: &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        },
        VerifyGate::Nextest => GateCommand {
            program: "cargo",
            args: &["nextest", "run", "--workspace", "--all-features"],
        },
        // `--all-features` matches the nextest gate so a doctest hidden behind a feature is
        // compiled rather than silently skipped, and `--workspace` so one crate cannot pass while
        // another carries broken examples.
        VerifyGate::Doctest => GateCommand {
            program: "cargo",
            args: &["test", "--workspace", "--all-features", "--doc"],
        },
        VerifyGate::Rustdoc => GateCommand {
            program: "cargo",
            args: &["doc", "--workspace", "--no-deps"],
        },
        VerifyGate::BootstrapCheck | VerifyGate::Conformance | VerifyGate::Architecture => {
            return None;
        }
    };
    Some(command)
}

pub(crate) fn run(root: &Path) -> Result<(), XtaskError> {
    struct HostRunner<'a> {
        root: &'a Path,
    }
    impl VerifyRunner for HostRunner<'_> {
        fn run(&self, gate: VerifyGate) -> Result<(), XtaskError> {
            match gate {
                VerifyGate::BootstrapCheck => bootstrap::check(self.root),
                VerifyGate::Conformance => conformance::check(self.root),
                VerifyGate::Architecture => architecture::check(self.root),
                _ => {
                    let Some(GateCommand { program, args }) = gate_command(gate) else {
                        return Err(XtaskError::Command(format!(
                            "gate {gate:?} names no command"
                        )));
                    };
                    let shell =
                        Shell::new().map_err(|error| XtaskError::Command(error.to_string()))?;
                    shell.change_dir(self.root);
                    cmd!(shell, "{program} {args...}")
                        .run()
                        .map_err(|error| XtaskError::Command(error.to_string()))
                }
            }
        }
    }
    verify_with(&HostRunner { root })
}
