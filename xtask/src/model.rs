//! Declarative Rulery workspace model.

pub(crate) const REQUIRED_PACKAGES: &[&str] = &[
    "rulery",
    "rulery-analysis",
    "rulery-cli",
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
    "rulery-vocabulary",
    "xtask",
];

pub(crate) const SCAFFOLD_CRATES: &[(&str, &str)] = &[
    ("crates/contracts", "rulery-contracts"),
    ("crates/diagnostics", "rulery-diagnostics"),
    ("crates/syntax", "rulery-syntax"),
    ("crates/vocabulary", "rulery-vocabulary"),
    ("crates/ir", "rulery-ir"),
    ("crates/compiler", "rulery-compiler"),
    ("crates/engine", "rulery-engine"),
    ("crates/analysis", "rulery-analysis"),
    ("crates/scenarios", "rulery-scenarios"),
    ("crates/emit", "rulery-emit"),
    ("crates/store", "rulery-store"),
    ("crates/cli", "rulery-cli"),
    ("crates/macros", "rulery-macros"),
    ("xtask", "xtask"),
];

pub(crate) fn manifest_path(package: &str) -> Option<&'static str> {
    match package {
        "rulery" => Some("Cargo.toml"),
        "rulery-contracts" => Some("crates/contracts/Cargo.toml"),
        "rulery-diagnostics" => Some("crates/diagnostics/Cargo.toml"),
        "rulery-syntax" => Some("crates/syntax/Cargo.toml"),
        "rulery-vocabulary" => Some("crates/vocabulary/Cargo.toml"),
        "rulery-ir" => Some("crates/ir/Cargo.toml"),
        "rulery-compiler" => Some("crates/compiler/Cargo.toml"),
        "rulery-engine" => Some("crates/engine/Cargo.toml"),
        "rulery-analysis" => Some("crates/analysis/Cargo.toml"),
        "rulery-scenarios" => Some("crates/scenarios/Cargo.toml"),
        "rulery-emit" => Some("crates/emit/Cargo.toml"),
        "rulery-store" => Some("crates/store/Cargo.toml"),
        "rulery-cli" => Some("crates/cli/Cargo.toml"),
        "rulery-macros" => Some("crates/macros/Cargo.toml"),
        "xtask" => Some("xtask/Cargo.toml"),
        _ => None,
    }
}

pub(crate) fn target_kind(package: &str) -> &'static str {
    match package {
        "rulery-cli" | "xtask" => "bin",
        "rulery-macros" => "proc-macro",
        _ => "lib",
    }
}

pub(crate) fn allowed_dependencies(package: &str) -> &'static [&'static str] {
    match package {
        "rulery-diagnostics" | "rulery-syntax" | "rulery-vocabulary" | "rulery-store" => {
            &["rulery-contracts"]
        }
        "rulery-ir" => &["rulery-contracts", "rulery-vocabulary"],
        "rulery-compiler" => &[
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-ir",
            "rulery-syntax",
            "rulery-vocabulary",
        ],
        "rulery-engine" => &["rulery-contracts", "rulery-ir"],
        "rulery-analysis" => &[
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-engine",
            "rulery-ir",
            "rulery-vocabulary",
        ],
        "rulery-scenarios" => &[
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-engine",
            "rulery-ir",
            "rulery-syntax",
        ],
        "rulery-emit" => &[
            "rulery-analysis",
            "rulery-contracts",
            "rulery-diagnostics",
            "rulery-engine",
            "rulery-ir",
            "rulery-scenarios",
        ],
        "rulery" => &[
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
            "rulery-vocabulary",
        ],
        "rulery-cli" => &["rulery"],
        _ => &[],
    }
}
