//! Write-capable command orchestration.

use std::path::Path;

use rulery::{LockMode, contracts::RulebookLock};

use crate::{Command, CommandOutput, ExitStatus};

/// Formatter result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatResult {
    /// Whether formatting would change files.
    pub changed: bool,
    /// Deterministically ordered affected paths.
    pub paths: Vec<String>,
}

/// Compilation result needed by check/lock orchestration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileResult {
    /// Whether parse, resolve, compile, and integrity validation succeeded.
    pub valid: bool,
    /// Human diagnostics.
    pub diagnostics: Vec<String>,
    /// Complete replacement lock when valid.
    pub proposed_lock: Option<RulebookLock>,
}

/// Filesystem and workflow ports for write-capable commands.
#[allow(clippy::missing_errors_doc)]
pub trait CommandPorts {
    /// Returns whether a destination exists and is non-empty.
    fn destination_non_empty(&self, path: &Path) -> Result<bool, String>;
    /// Initializes an empty package destination.
    fn init(&self, path: &Path) -> Result<(), String>;
    /// Computes canonical formatting.
    fn format(&self, path: &Path, write: bool) -> Result<FormatResult, String>;
    /// Compiles a package under explicit lock policy.
    fn compile(&self, path: &Path, mode: LockMode) -> Result<CompileResult, String>;
    /// Atomically writes a validated replacement lock.
    fn write_lock(&self, path: &Path, lock: &RulebookLock) -> Result<(), String>;
}

/// Runs init, fmt, check, or lock without implicit writes.
#[must_use]
pub fn run_write_command<P: CommandPorts>(command: &Command, ports: &P) -> CommandOutput {
    match command {
        Command::Init { path } => match ports.destination_non_empty(path) {
            Ok(true) => output(
                ExitStatus::InvalidInvocation,
                Vec::new(),
                b"destination is not empty\n".to_vec(),
            ),
            Ok(false) => match ports.init(path) {
                Ok(()) => output(ExitStatus::Success, Vec::new(), Vec::new()),
                Err(error) => io_error(&error),
            },
            Err(error) => io_error(&error),
        },
        Command::Fmt { path, check } => match ports.format(path, !check) {
            Ok(result) => {
                let mut stdout = result.paths.join("\n").into_bytes();
                if !stdout.is_empty() {
                    stdout.push(b'\n');
                }
                let status = if *check && result.changed {
                    ExitStatus::DiagnosticsError
                } else {
                    ExitStatus::Success
                };
                output(status, stdout, Vec::new())
            }
            Err(error) => io_error(&error),
        },
        Command::Check { path, frozen, .. } => {
            let mode = if *frozen {
                LockMode::Frozen
            } else {
                LockMode::Update
            };
            match ports.compile(path, mode) {
                Ok(result) if result.valid => output(ExitStatus::Success, Vec::new(), Vec::new()),
                Ok(result) => diagnostics_error(&result.diagnostics),
                Err(error) => internal_error(&error),
            }
        }
        Command::Lock { path } => match ports.compile(path, LockMode::Update) {
            Ok(result) if !result.valid => diagnostics_error(&result.diagnostics),
            Ok(result) => match result.proposed_lock {
                Some(lock) => match ports.write_lock(path, &lock) {
                    Ok(()) => output(ExitStatus::Success, Vec::new(), Vec::new()),
                    Err(error) => io_error(&error),
                },
                None => internal_error("valid lock workflow produced no lock"),
            },
            Err(error) => internal_error(&error),
        },
        Command::Analyze { .. }
        | Command::Test { .. }
        | Command::Explain { .. }
        | Command::Diff { .. }
        | Command::Render { .. } => output(
            ExitStatus::InvalidInvocation,
            Vec::new(),
            b"command is not a write workflow\n".to_vec(),
        ),
    }
}

fn output(status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8>) -> CommandOutput {
    CommandOutput::new(status, stdout, stderr)
}

fn diagnostics_error(diagnostics: &[String]) -> CommandOutput {
    let mut stderr = diagnostics.join("\n").into_bytes();
    if !stderr.is_empty() {
        stderr.push(b'\n');
    }
    output(ExitStatus::DiagnosticsError, Vec::new(), stderr)
}

fn io_error(error: &str) -> CommandOutput {
    output(
        ExitStatus::IoFailure,
        Vec::new(),
        format!("{error}\n").into_bytes(),
    )
}

fn internal_error(error: &str) -> CommandOutput {
    output(
        ExitStatus::InternalFailure,
        Vec::new(),
        format!("{error}\n").into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::path::{Path, PathBuf};

    use rulery::contracts::{
        ContentHash, LanguageVersion, LockedRoot, NormalizedSourceLocation, PackageId,
        RulebookLock, Version,
    };

    use crate::{Command, OutputFormat};

    use super::*;

    #[test]
    fn write_commands_obey_scope_and_atomicity() {
        let ports = FakePorts::default();
        let init = run_write_command(
            &Command::Init {
                path: PathBuf::from("new"),
            },
            &ports,
        );
        assert_eq!(init.status, ExitStatus::Success);
        assert_eq!(ports.init_writes.get(), 1);

        ports.non_empty.set(true);
        let rejected = run_write_command(
            &Command::Init {
                path: PathBuf::from("existing"),
            },
            &ports,
        );
        assert_eq!(rejected.status, ExitStatus::InvalidInvocation);
        assert_eq!(ports.init_writes.get(), 1);
        ports.non_empty.set(false);

        let checked = run_write_command(
            &Command::Fmt {
                path: PathBuf::from("."),
                check: true,
            },
            &ports,
        );
        assert_eq!(checked.status, ExitStatus::DiagnosticsError);
        assert_eq!(ports.format_writes.get(), 0);
        assert_eq!(
            String::from_utf8(checked.stdout)
                .expect("stdout")
                .lines()
                .count(),
            2
        );
        let formatted = run_write_command(
            &Command::Fmt {
                path: PathBuf::from("."),
                check: false,
            },
            &ports,
        );
        assert_eq!(formatted.status, ExitStatus::Success);
        assert_eq!(ports.format_writes.get(), 1);

        let check = run_write_command(
            &Command::Check {
                path: PathBuf::from("."),
                frozen: false,
                deny_warnings: false,
                format: OutputFormat::Human,
            },
            &ports,
        );
        assert_eq!(check.status, ExitStatus::Success);
        assert_eq!(ports.lock_writes.get(), 0);
        assert_eq!(ports.modes.borrow().as_slice(), &[LockMode::Update]);

        let lock = run_write_command(
            &Command::Lock {
                path: PathBuf::from("."),
            },
            &ports,
        );
        assert_eq!(lock.status, ExitStatus::Success);
        assert_eq!(ports.lock_writes.get(), 1);
        assert_eq!(ports.modes.borrow().last(), Some(&LockMode::Update));

        ports.fail_atomic.set(true);
        let failed = run_write_command(
            &Command::Lock {
                path: PathBuf::from("."),
            },
            &ports,
        );
        assert_eq!(failed.status, ExitStatus::IoFailure);
        assert_eq!(ports.lock_writes.get(), 1);

        ports.valid.set(false);
        let invalid = run_write_command(
            &Command::Lock {
                path: PathBuf::from("."),
            },
            &ports,
        );
        assert_eq!(invalid.status, ExitStatus::DiagnosticsError);
        assert_eq!(ports.lock_writes.get(), 1);
    }

    struct FakePorts {
        non_empty: Cell<bool>,
        init_writes: Cell<u32>,
        format_writes: Cell<u32>,
        lock_writes: Cell<u32>,
        fail_atomic: Cell<bool>,
        valid: Cell<bool>,
        modes: RefCell<Vec<LockMode>>,
    }

    impl Default for FakePorts {
        fn default() -> Self {
            Self {
                non_empty: Cell::new(false),
                init_writes: Cell::new(0),
                format_writes: Cell::new(0),
                lock_writes: Cell::new(0),
                fail_atomic: Cell::new(false),
                valid: Cell::new(true),
                modes: RefCell::new(Vec::new()),
            }
        }
    }

    impl CommandPorts for FakePorts {
        fn destination_non_empty(&self, _: &Path) -> Result<bool, String> {
            Ok(self.non_empty.get())
        }
        fn init(&self, _: &Path) -> Result<(), String> {
            self.init_writes.set(self.init_writes.get() + 1);
            Ok(())
        }
        fn format(&self, _: &Path, write: bool) -> Result<FormatResult, String> {
            if write {
                self.format_writes.set(self.format_writes.get() + 1);
            }
            Ok(FormatResult {
                changed: true,
                paths: vec!["a.yaml".to_owned(), "b.yaml".to_owned()],
            })
        }
        fn compile(&self, _: &Path, mode: LockMode) -> Result<CompileResult, String> {
            self.modes.borrow_mut().push(mode);
            Ok(CompileResult {
                valid: self.valid.get(),
                diagnostics: (!self.valid.get())
                    .then(|| "invalid".to_owned())
                    .into_iter()
                    .collect(),
                proposed_lock: self.valid.get().then(sample_lock),
            })
        }
        fn write_lock(&self, _: &Path, _: &RulebookLock) -> Result<(), String> {
            if self.fail_atomic.get() {
                return Err("atomic failure".to_owned());
            }
            self.lock_writes.set(self.lock_writes.get() + 1);
            Ok(())
        }
    }

    fn sample_lock() -> RulebookLock {
        RulebookLock::new(
            LockedRoot::new(
                PackageId::new("pkg.main").expect("package"),
                Version::new("1.0.0").expect("version"),
                NormalizedSourceLocation::new("root").expect("source"),
                ContentHash::from_bytes([1; 32]),
            ),
            LanguageVersion::V1,
            Vec::new(),
        )
        .expect("lock")
    }
}
