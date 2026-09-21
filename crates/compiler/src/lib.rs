//! Validation and deterministic lowering for Rulery rulebooks.
//!
//! Compiler passes resolve symbols, validate references, type-check predicates and actions,
//! normalize conditions, compute static specificity, and lower checked declarations into
//! `rulery.compiled-package/v1`. [`CompilationOutput`] deliberately omits the package whenever an
//! error diagnostic exists; callers must not evaluate partially checked input.

#![forbid(unsafe_code)]

mod lower;
mod normalize;
mod resolve;
mod typecheck;
mod validate;

use rulery_contracts::{
    ActionId, ContentHash, DecisionId, FactPath, LanguageVersion, PackageId, RuleId, SourceMap,
    StableId, TypeId, Version,
};
use rulery_diagnostics::DiagnosticCode;
use rulery_ir::{
    CompilationInput as PackageCompilationInput, CompiledPackage, CompiledPackageDraft,
    PackageIntegritySet,
};
use rulery_vocabulary::ResolvedVocabulary;

pub use lower::{LoweringInput, lower_package};
pub use normalize::{
    NormalizationError, NormalizationOutput, normalize_condition, specificity_from_counts,
};
pub use resolve::{ResolvedSymbols, SymbolError, resolve_symbols};
pub use typecheck::{
    ActionCallSpec, ActionParamSpec, CheckedType, OperandSpec, TypecheckError, typecheck_action,
    typecheck_predicate,
};
pub use validate::{ValidationIssue, validate_symbols};

/// One authored rule binding used for symbol validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleBinding {
    /// Label describing the source provenance for this rule.
    pub source_label: String,
    /// Declared decision identity.
    pub decision: DecisionId,
    /// Rule identity.
    pub rule: RuleId,
    /// Referenced fact paths.
    pub fact_paths: Vec<FactPath>,
    /// Invoked actions.
    pub action_calls: Vec<ActionId>,
    /// Referenced operational terms.
    pub terms: Vec<StableId>,
    /// Optional override target (`package::rule`).
    pub override_target: Option<String>,
    /// Optional rationale when `override_target` is present.
    pub override_rationale: Option<String>,
}

/// Symbol-validation compiler input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationInput {
    /// Package identity.
    pub package_id: PackageId,
    /// Package semantic version.
    pub package_version: Version,
    /// Language version.
    pub language_version: LanguageVersion,
    /// Compiler identity.
    pub compiler_identity: String,
    /// Source map retained for compiled package construction.
    pub source_map: SourceMap,
    /// Resolved vocabulary retained for compiled package construction.
    pub vocabulary: ResolvedVocabulary,
    /// Declared decisions.
    pub decisions: Vec<DecisionId>,
    /// Declared rules.
    pub rules: Vec<RuleBinding>,
    /// Declared actions.
    pub actions: Vec<ActionId>,
    /// Declared types.
    pub types: Vec<TypeId>,
    /// Available fact paths.
    pub available_fact_paths: Vec<FactPath>,
    /// Available terms.
    pub available_terms: Vec<StableId>,
    /// Imported package ids available for qualification.
    pub imported_packages: Vec<PackageId>,
}

/// Minimal compiler provenance evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceEvidence {
    /// Package identity under compilation.
    pub package_id: PackageId,
    /// Compiler identity.
    pub compiler_identity: String,
    /// Language version.
    pub language_version: LanguageVersion,
}

/// Compiler-produced diagnostic entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable message.
    pub message: String,
    /// Source provenance label.
    pub source_label: String,
    /// Required provenance evidence.
    pub provenance: ProvenanceEvidence,
}

/// Compiler output with optional compiled package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationOutput {
    /// Compiled package when no error diagnostics exist.
    pub package: Option<CompiledPackage>,
    /// Deterministically ordered diagnostics.
    pub diagnostics: Vec<CompilerDiagnostic>,
}

/// Symbol validation compiler.
#[derive(Clone, Debug, Default)]
pub struct PolicyCompiler;

impl PolicyCompiler {
    /// Compiles and validates symbol declarations.
    #[must_use]
    pub fn compile(&self, input: &CompilationInput) -> CompilationOutput {
        let resolved = resolve_symbols(input);
        let mut diagnostics = validate_symbols(input, &resolved)
            .into_iter()
            .map(|issue| issue.into_diagnostic(input))
            .collect::<Vec<_>>();
        diagnostics.sort_by(|left, right| {
            left.code
                .as_str()
                .cmp(right.code.as_str())
                .then_with(|| left.source_label.cmp(&right.source_label))
                .then_with(|| left.message.cmp(&right.message))
        });

        let has_error = diagnostics
            .iter()
            .any(|entry| entry.code.as_str() != DiagnosticCode::UNDEFINED_OPERATIONAL_TERM);

        let package = if has_error {
            None
        } else {
            build_empty_package(input)
        };

        CompilationOutput {
            package,
            diagnostics,
        }
    }
}

fn build_empty_package(input: &CompilationInput) -> Option<CompiledPackage> {
    let draft = CompiledPackageDraft {
        package_id: input.package_id.clone(),
        package_version: input.package_version.clone(),
        language_version: input.language_version,
        compiler_identity: input.compiler_identity.clone(),
        decisions: Vec::new(),
        actions: Vec::new(),
        integrity: PackageIntegritySet::new(PackageCompilationInput::new(
            ContentHash::from_bytes([0; 32]),
            ContentHash::from_bytes([0; 32]),
            None,
        )),
    };
    CompiledPackage::new(draft, input.source_map.clone(), input.vocabulary.clone()).ok()
}

impl ValidationIssue {
    fn into_diagnostic(self, input: &CompilationInput) -> CompilerDiagnostic {
        CompilerDiagnostic {
            code: self.code,
            message: self.message,
            source_label: self.source_label,
            provenance: ProvenanceEvidence {
                package_id: input.package_id.clone(),
                compiler_identity: input.compiler_identity.clone(),
                language_version: input.language_version,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use rulery_contracts::{
        ActionId, DecisionId, FactPath, PackageId, RuleId, SourceFile, SourceId, SourceKey,
        SourceMap, SourcePath, StableId, TypeId,
    };

    use super::*;

    #[test]
    fn compiler_rejects_duplicate_and_unknown_symbols() {
        let input = CompilationInput {
            package_id: PackageId::new("pkg.main").expect("package"),
            package_version: Version::new("1.0.0").expect("version"),
            language_version: LanguageVersion::V1,
            compiler_identity: "rulery.compiler".to_owned(),
            source_map: sample_source_map(),
            vocabulary: ResolvedVocabulary::default(),
            decisions: vec![
                DecisionId::new("decision.authz").expect("decision"),
                DecisionId::new("decision.authz").expect("decision duplicate"),
            ],
            rules: vec![
                RuleBinding {
                    source_label: "rulery.yaml:decision.authz.rule.allow".to_owned(),
                    decision: DecisionId::new("decision.authz").expect("decision"),
                    rule: RuleId::new("rule.allow").expect("rule"),
                    fact_paths: vec![FactPath::from_str("account.status").expect("fact path")],
                    action_calls: vec![ActionId::new("action.notify").expect("action")],
                    terms: vec![StableId::new("term.unknown").expect("term")],
                    override_target: Some("pkg.unknown::rule.base".to_owned()),
                    override_rationale: None,
                },
                RuleBinding {
                    source_label: "rules/secondary.yaml:decision.ghost.rule.shadow".to_owned(),
                    decision: DecisionId::new("decision.ghost").expect("ghost decision"),
                    rule: RuleId::new("rule.shadow").expect("rule"),
                    fact_paths: vec![FactPath::from_str("ghost.fact").expect("fact path")],
                    action_calls: vec![ActionId::new("action.missing").expect("action")],
                    terms: vec![StableId::new("term.temporal.deadline").expect("term")],
                    override_target: None,
                    override_rationale: None,
                },
            ],
            actions: vec![
                ActionId::new("action.notify").expect("action"),
                ActionId::new("action.notify").expect("action duplicate"),
            ],
            types: vec![
                TypeId::new("type.account").expect("type"),
                TypeId::new("type.account").expect("type duplicate"),
            ],
            available_fact_paths: vec![FactPath::from_str("account.status").expect("fact path")],
            available_terms: Vec::new(),
            imported_packages: vec![PackageId::new("pkg.shared").expect("imported package")],
        };

        let output = PolicyCompiler.compile(&input);
        let codes = output
            .diagnostics
            .iter()
            .map(|entry| entry.code.as_str().to_owned())
            .collect::<Vec<_>>();

        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::DUPLICATE_DECLARATION)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::UNKNOWN_SYMBOL)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::INVALID_FACT_PATH)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::UNDEFINED_OPERATIONAL_TERM)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::TEMPORAL_TERM_UNDEFINED)
        );

        for diagnostic in &output.diagnostics {
            assert!(!diagnostic.source_label.is_empty());
            assert_eq!(diagnostic.provenance.package_id, input.package_id);
            assert_eq!(diagnostic.provenance.language_version, LanguageVersion::V1);
            assert_eq!(diagnostic.provenance.compiler_identity, "rulery.compiler");
        }

        let mut sorted = output.diagnostics.clone();
        sorted.sort_by(|left, right| {
            left.code
                .as_str()
                .cmp(right.code.as_str())
                .then_with(|| left.source_label.cmp(&right.source_label))
                .then_with(|| left.message.cmp(&right.message))
        });
        assert_eq!(output.diagnostics, sorted);
        assert!(output.package.is_none());
    }

    fn sample_source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("src.main").expect("source id"),
                SourcePath::new("rulery.yaml").expect("path"),
                Arc::<str>::from("content"),
            ),
        )
        .expect("insert source");
        map
    }
}
