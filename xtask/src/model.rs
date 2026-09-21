//! Declarative Rulery workspace model.

/// Cargo target kind required by a modeled package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetKind {
    /// Rust library.
    Library,
    /// Executable binary.
    Binary,
    /// Procedural macro library.
    ProcMacro,
}

/// Allowed internal dependency rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DependencyRule {
    /// Package receiving dependencies.
    pub package: &'static str,
    /// Exact allowed internal package names.
    pub allowed: &'static [&'static str],
}

/// One modeled workspace package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CrateSpec {
    /// Cargo package name.
    pub name: &'static str,
    /// Package directory relative to workspace root.
    pub path: &'static str,
    /// Manifest path relative to workspace root.
    pub manifest: &'static str,
    /// Required target kind.
    pub target: TargetKind,
    /// Dependency allowlist.
    pub dependencies: DependencyRule,
}

/// Single declarative workspace architecture authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceModel {
    /// Workspace root marker.
    pub root: &'static str,
    /// Every modeled package.
    pub crates: &'static [CrateSpec],
}

macro_rules! spec {
    ($name:literal, $path:literal, $manifest:literal, $target:ident, [$($dependency:literal),* $(,)?]) => {
        CrateSpec {
            name: $name,
            path: $path,
            manifest: $manifest,
            target: TargetKind::$target,
            dependencies: DependencyRule { package: $name, allowed: &[$($dependency),*] },
        }
    };
}

const CRATES: &[CrateSpec] = &[
    spec!(
        "rulery",
        ".",
        "Cargo.toml",
        Library,
        [
            "rulery-analysis",
            "rulery-compiler",
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-emit",
            "rulery-engine",
            "rulery-ir",
            "rulery-macros",
            "rulery-scenarios",
            "rulery-store",
            "rulery-syntax",
            "rulery-vocabulary"
        ]
    ),
    spec!(
        "rulery-contracts",
        "crates/contracts",
        "crates/contracts/Cargo.toml",
        Library,
        []
    ),
    spec!(
        "rulery-diagnostics",
        "crates/diagnostics",
        "crates/diagnostics/Cargo.toml",
        Library,
        ["rulery-contracts"]
    ),
    spec!(
        "rulery-syntax",
        "crates/syntax",
        "crates/syntax/Cargo.toml",
        Library,
        ["rulery-contracts"]
    ),
    spec!(
        "rulery-vocabulary",
        "crates/vocabulary",
        "crates/vocabulary/Cargo.toml",
        Library,
        ["rulery-contracts"]
    ),
    spec!(
        "rulery-ir",
        "crates/ir",
        "crates/ir/Cargo.toml",
        Library,
        ["rulery-contracts", "rulery-vocabulary"]
    ),
    spec!(
        "rulery-compiler",
        "crates/compiler",
        "crates/compiler/Cargo.toml",
        Library,
        [
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-ir",
            "rulery-syntax",
            "rulery-vocabulary"
        ]
    ),
    spec!(
        "rulery-engine",
        "crates/engine",
        "crates/engine/Cargo.toml",
        Library,
        ["rulery-contracts", "rulery-ir"]
    ),
    spec!(
        "rulery-analysis",
        "crates/analysis",
        "crates/analysis/Cargo.toml",
        Library,
        [
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-engine",
            "rulery-ir",
            "rulery-vocabulary"
        ]
    ),
    spec!(
        "rulery-scenarios",
        "crates/scenarios",
        "crates/scenarios/Cargo.toml",
        Library,
        [
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-engine",
            "rulery-ir",
            "rulery-syntax"
        ]
    ),
    spec!(
        "rulery-emit",
        "crates/emit",
        "crates/emit/Cargo.toml",
        Library,
        [
            "rulery-analysis",
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-engine",
            "rulery-ir",
            "rulery-scenarios"
        ]
    ),
    spec!(
        "rulery-store",
        "crates/store",
        "crates/store/Cargo.toml",
        Library,
        ["rulery-contracts"]
    ),
    spec!(
        "rulery-cli",
        "crates/cli",
        "crates/cli/Cargo.toml",
        Binary,
        ["rulery"]
    ),
    spec!(
        "rulery-macros",
        "crates/macros",
        "crates/macros/Cargo.toml",
        ProcMacro,
        []
    ),
    spec!("xtask", "xtask", "xtask/Cargo.toml", Binary, []),
];

/// Returns the declarative workspace model.
#[must_use]
pub const fn workspace_model() -> WorkspaceModel {
    WorkspaceModel {
        root: ".",
        crates: CRATES,
    }
}

fn crate_spec(package: &str) -> Option<&'static CrateSpec> {
    CRATES.iter().find(|spec| spec.name == package)
}

/// Returns a modeled manifest path by package name.
#[must_use]
pub fn model_manifest(package: &str) -> Option<&'static str> {
    crate_spec(package).map(|spec| spec.manifest)
}

/// Returns a modeled target kind by package name.
#[must_use]
pub fn model_target(package: &str) -> Option<TargetKind> {
    crate_spec(package).map(|spec| spec.target)
}

/// Returns modeled internal dependencies by package name.
#[must_use]
pub fn model_dependencies(package: &str) -> Option<&'static [&'static str]> {
    crate_spec(package).map(|spec| spec.dependencies.allowed)
}
