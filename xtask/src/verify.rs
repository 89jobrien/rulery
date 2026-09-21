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
    /// Workspace rustdoc build.
    Rustdoc,
}

/// Injected gate runner.
#[allow(clippy::missing_errors_doc)]
pub trait VerifyRunner {
    /// Runs one gate.
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
        VerifyGate::Rustdoc,
    ] {
        runner.run(gate)?;
    }
    Ok(())
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
                VerifyGate::Format
                | VerifyGate::Clippy
                | VerifyGate::Nextest
                | VerifyGate::Rustdoc => {
                    let shell =
                        Shell::new().map_err(|error| XtaskError::Command(error.to_string()))?;
                    shell.change_dir(self.root);
                    let result = match gate {
                        VerifyGate::Format => cmd!(shell, "cargo fmt --all --check").run(),
                        VerifyGate::Clippy => {
                            cmd!(shell, "cargo clippy --workspace -- -D warnings").run()
                        }
                        VerifyGate::Nextest => cmd!(shell, "cargo nextest run --workspace").run(),
                        VerifyGate::Rustdoc => cmd!(shell, "cargo doc --workspace --no-deps").run(),
                        _ => unreachable!(),
                    };
                    result.map_err(|error| XtaskError::Command(error.to_string()))
                }
            }
        }
    }
    verify_with(&HostRunner { root })
}
