//! Deterministic lowering into compiled package wire data.

use rulery_contracts::SourceMap;
use rulery_diagnostics::{Severity, diagnostic_definition};
use rulery_ir::{CompiledPackage, CompiledPackageDraft};
use rulery_vocabulary::ResolvedVocabulary;

use crate::{CompilationOutput, CompilerDiagnostic};

/// Fully checked input ready for deterministic package lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweringInput {
    /// Checked package declarations and metadata.
    pub draft: CompiledPackageDraft,
    /// Complete remapped source catalog.
    pub source_map: SourceMap,
    /// Complete resolved vocabulary.
    pub vocabulary: ResolvedVocabulary,
    /// Diagnostics accumulated before lowering.
    pub diagnostics: Vec<CompilerDiagnostic>,
}

/// Lowers a checked package unless an error diagnostic blocks output.
#[must_use]
pub fn lower_package(mut input: LoweringInput) -> CompilationOutput {
    input.diagnostics.sort_by(|left, right| {
        left.code
            .as_str()
            .cmp(right.code.as_str())
            .then_with(|| left.source_label.cmp(&right.source_label))
            .then_with(|| left.message.cmp(&right.message))
    });

    let has_error = input.diagnostics.iter().any(|diagnostic| {
        diagnostic_definition(&diagnostic.code)
            .is_none_or(|definition| definition.severity() == Severity::Error)
    });
    let package = if has_error {
        None
    } else {
        CompiledPackage::new(input.draft, input.source_map, input.vocabulary).ok()
    };

    CompilationOutput {
        package,
        diagnostics: input.diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rulery_contracts::{
        ContentHash, HashDomain, LanguageVersion, PackageId, SourceFile, SourceId, SourceKey,
        SourceMap, SourcePath, Version, hash_parts,
    };
    use rulery_diagnostics::{DiagnosticCode, Severity, diagnostic_definition};
    use rulery_ir::{
        CompilationInput as PackageCompilationInput, CompiledPackageDraft, PackageIntegritySet,
    };
    use rulery_vocabulary::ResolvedVocabulary;

    use crate::{CompilerDiagnostic, ProvenanceEvidence};

    use super::*;

    #[test]
    fn compilation_is_byte_deterministic() {
        let forward = lower_package(input(source_map(false), Vec::new()));
        let reverse = lower_package(input(source_map(true), Vec::new()));
        let forward_package = forward.package.expect("forward package");
        let reverse_package = reverse.package.expect("reverse package");

        assert_eq!(
            forward_package.canonical_bytes().expect("forward bytes"),
            reverse_package.canonical_bytes().expect("reverse bytes")
        );
        assert_eq!(
            forward_package.payload().package_hash(),
            reverse_package.payload().package_hash()
        );

        let hash_input = forward_package.hash_input_bytes().expect("hash input");
        assert_eq!(
            forward_package.payload().package_hash(),
            hash_parts(HashDomain::CompiledPackageV1, [hash_input.as_slice()])
        );
        let complete = serde_json::to_value(forward_package.payload()).expect("payload json");
        for field in [
            "package_id",
            "package_version",
            "vocabulary",
            "decisions",
            "actions",
            "source_map",
            "integrity",
            "compiler_identity",
            "language_version",
            "package_hash",
        ] {
            assert!(complete.get(field).is_some(), "missing field {field}");
        }

        let error = CompilerDiagnostic {
            code: DiagnosticCode::new(DiagnosticCode::UNKNOWN_SYMBOL).expect("code"),
            message: "unknown symbol".to_owned(),
            source_label: "rules/main.yaml".to_owned(),
            provenance: provenance(),
        };
        assert_eq!(
            diagnostic_definition(&error.code).map(|definition| definition.severity()),
            Some(Severity::Error)
        );
        let rejected = lower_package(input(source_map(false), vec![error]));
        assert!(rejected.package.is_none());
    }

    fn input(source_map: SourceMap, diagnostics: Vec<CompilerDiagnostic>) -> LoweringInput {
        LoweringInput {
            draft: CompiledPackageDraft {
                package_id: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "rulery.compiler/0.1.0".to_owned(),
                decisions: Vec::new(),
                actions: Vec::new(),
                integrity: PackageIntegritySet::new(PackageCompilationInput::new(
                    ContentHash::from_bytes([1; 32]),
                    ContentHash::from_bytes([2; 32]),
                    Some(ContentHash::from_bytes([3; 32])),
                )),
            },
            source_map,
            vocabulary: ResolvedVocabulary::default(),
            diagnostics,
        }
    }

    fn source_map(reverse: bool) -> SourceMap {
        let entries = if reverse {
            vec![(2, "src.b", "rules/b.yaml"), (1, "src.a", "rules/a.yaml")]
        } else {
            vec![(1, "src.a", "rules/a.yaml"), (2, "src.b", "rules/b.yaml")]
        };
        let mut map = SourceMap::new();
        for (key, id, path) in entries {
            map.insert(
                SourceKey::new(key),
                SourceFile::new(
                    SourceId::new(id).expect("source id"),
                    SourcePath::new(path).expect("source path"),
                    Arc::<str>::from("rule: true\n"),
                ),
            )
            .expect("source insert");
        }
        map
    }

    fn provenance() -> ProvenanceEvidence {
        ProvenanceEvidence {
            package_id: PackageId::new("pkg.main").expect("package"),
            compiler_identity: "rulery.compiler/0.1.0".to_owned(),
            language_version: LanguageVersion::V1,
        }
    }
}
