//! Exact v0.1 CLI argument surface.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rulery::contracts::DecisionId;

/// Rulery command-line parser.
#[derive(Clone, Debug, Parser, PartialEq)]
#[command(name = "rulery")]
pub struct Cli {
    /// Selected command.
    #[command(subcommand)]
    pub command: Command,
}

/// Rulery command.
#[derive(Clone, Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Initialize a package.
    Init {
        /// Destination path.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Format source documents.
    Fmt {
        /// Package or file path.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Check formatting without writes.
        #[arg(long)]
        check: bool,
    },
    /// Validate a package.
    Check {
        /// Package path.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Require exact lock agreement.
        #[arg(long)]
        frozen: bool,
        /// Promote eligible warnings.
        #[arg(long)]
        deny_warnings: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Analyze a package.
    Analyze {
        /// Package path.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Require exact lock agreement.
        #[arg(long)]
        frozen: bool,
        /// Promote eligible warnings.
        #[arg(long)]
        deny_warnings: bool,
        /// State budget.
        #[arg(long, default_value_t = 100_000)]
        max_states: u64,
        /// Witness budget.
        #[arg(long, default_value_t = 1_000)]
        max_witnesses: u32,
        /// Output format.
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Run root scenarios.
    Test {
        /// Package path.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Require exact lock agreement.
        #[arg(long)]
        frozen: bool,
        /// Promote eligible warnings.
        #[arg(long)]
        deny_warnings: bool,
        /// Optional tag or scenario filter.
        #[arg(long)]
        filter: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t)]
        format: BasicFormat,
    },
    /// Explain one decision.
    Explain {
        /// Package path.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Decision identity.
        #[arg(long)]
        decision: DecisionId,
        /// Facts file.
        #[arg(long)]
        facts: PathBuf,
        /// Fixed RFC3339 instant; absent means system UTC.
        #[arg(long)]
        at: Option<String>,
        /// Require exact lock agreement.
        #[arg(long)]
        frozen: bool,
        /// Promote eligible warnings.
        #[arg(long)]
        deny_warnings: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t)]
        format: BasicFormat,
    },
    /// Semantically diff two packages.
    Diff {
        /// Before package path.
        before: PathBuf,
        /// After package path.
        after: PathBuf,
        /// Optional decision filter.
        #[arg(long)]
        decision: Option<DecisionId>,
        /// State budget.
        #[arg(long, default_value_t = 100_000)]
        max_states: u64,
        /// Witness budget.
        #[arg(long, default_value_t = 1_000)]
        max_witnesses: u32,
        /// Require exact lock agreement.
        #[arg(long)]
        frozen: bool,
        /// Promote eligible warnings.
        #[arg(long)]
        deny_warnings: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t)]
        format: BasicFormat,
    },
    /// Render policy artifacts.
    Render {
        /// Package path.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Render format.
        #[arg(long, value_enum)]
        format: RenderFormat,
        /// Optional decision; required for decision tables.
        #[arg(long, required_if_eq("format", "decision-table"))]
        decision: Option<DecisionId>,
        /// Require exact lock agreement.
        #[arg(long)]
        frozen: bool,
        /// Promote eligible warnings.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Resolve and atomically write the complete lock.
    Lock {
        /// Package path.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

impl Command {
    /// Returns assembly lock mode, or `None` for non-assembly commands.
    #[must_use]
    pub const fn lock_mode(&self) -> Option<rulery::LockMode> {
        let frozen = match self {
            Self::Init { .. } | Self::Fmt { .. } => return None,
            Self::Lock { .. } => return Some(rulery::LockMode::Update),
            Self::Check { frozen, .. }
            | Self::Analyze { frozen, .. }
            | Self::Test { frozen, .. }
            | Self::Explain { frozen, .. }
            | Self::Diff { frozen, .. }
            | Self::Render { frozen, .. } => *frozen,
        };
        Some(if frozen {
            rulery::LockMode::Frozen
        } else {
            rulery::LockMode::Update
        })
    }
}

/// General output format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    /// Human text.
    #[default]
    Human,
    /// JSON.
    Json,
    /// SARIF.
    Sarif,
}

/// Render output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RenderFormat {
    /// Markdown.
    Markdown,
    /// JSON.
    Json,
    /// Decision table.
    DecisionTable,
}

/// Human or JSON output format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum BasicFormat {
    /// Human text.
    #[default]
    Human,
    /// JSON.
    Json,
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use rulery::contracts::DecisionId;

    use super::*;

    #[test]
    fn cli_parses_v01_commands_and_defaults() {
        let cases = [
            vec!["rulery", "init"],
            vec!["rulery", "fmt"],
            vec!["rulery", "check"],
            vec!["rulery", "analyze"],
            vec!["rulery", "test"],
            vec![
                "rulery",
                "explain",
                "--decision",
                "decision.main",
                "--facts",
                "facts.json",
            ],
            vec!["rulery", "diff", "before", "after"],
            vec!["rulery", "render", "--format", "markdown"],
            vec!["rulery", "lock"],
        ];
        for args in cases {
            assert!(Cli::try_parse_from(args).is_ok());
        }

        assert!(
            matches!(Cli::try_parse_from(["rulery", "check"]).expect("check").command, Command::Check { path, frozen: false, deny_warnings: false, format: OutputFormat::Human } if path == PathBuf::from("."))
        );
        assert!(matches!(
            Cli::try_parse_from(["rulery", "analyze"])
                .expect("analyze")
                .command,
            Command::Analyze {
                max_states: 100_000,
                max_witnesses: 1_000,
                frozen: false,
                ..
            }
        ));
        assert!(
            matches!(Cli::try_parse_from(["rulery", "explain", "--decision", "decision.main", "--facts", "facts.json"]).expect("explain").command, Command::Explain { decision, at: None, frozen: false, .. } if decision == DecisionId::new("decision.main").expect("id"))
        );
        assert!(
            matches!(Cli::try_parse_from(["rulery", "test", "--filter", "smoke"]).expect("test").command, Command::Test { filter: Some(value), .. } if value == "smoke")
        );
        assert!(matches!(
            Cli::try_parse_from(["rulery", "check", "--frozen"])
                .expect("frozen")
                .command,
            Command::Check { frozen: true, .. }
        ));
        assert_eq!(
            Cli::try_parse_from(["rulery", "check"])
                .expect("check")
                .command
                .lock_mode(),
            Some(rulery::LockMode::Update)
        );
        assert_eq!(
            Cli::try_parse_from(["rulery", "check", "--frozen"])
                .expect("frozen")
                .command
                .lock_mode(),
            Some(rulery::LockMode::Frozen)
        );
        assert_eq!(
            Cli::try_parse_from(["rulery", "lock"])
                .expect("lock")
                .command
                .lock_mode(),
            Some(rulery::LockMode::Update)
        );

        for args in [
            vec!["rulery", "check", "--format", "markdown"],
            vec![
                "rulery",
                "explain",
                "--decision",
                "INVALID",
                "--facts",
                "facts.json",
            ],
            vec!["rulery", "render", "--format", "decision-table"],
            vec!["rulery", "render", "--format", "sarif"],
        ] {
            assert_eq!(
                Cli::try_parse_from(args).expect_err("invalid").exit_code(),
                2
            );
        }
    }
}
