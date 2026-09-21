//! Checked compiled package model.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use rulery_contracts::{
    ActionId, ContentHash, DecisionId, HashDomain, LanguageVersion, PackageId, QualifiedRuleId,
    RuleId, SourceMap, Span, StableId, Version, hash_parts,
};
use rulery_vocabulary::{ResolvedVocabulary, TypeDeclaration};

use crate::Expr;

/// One checked action binding retained in a compiled package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledAction {
    id: ActionId,
    parameters: BTreeMap<StableId, CompiledActionParameter>,
}

impl CompiledAction {
    /// Creates a compiled action declaration.
    #[must_use]
    pub fn new(id: ActionId, parameters: BTreeMap<StableId, CompiledActionParameter>) -> Self {
        Self { id, parameters }
    }

    /// Returns the action identifier.
    #[must_use]
    pub fn id(&self) -> &ActionId {
        &self.id
    }
}

/// One compiled action parameter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledActionParameter {
    required: bool,
}

impl CompiledActionParameter {
    /// Creates a compiled action parameter.
    #[must_use]
    pub const fn new(required: bool) -> Self {
        Self { required }
    }
}

/// One checked compiled rule.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledRule {
    id: RuleId,
    qualified_id: QualifiedRuleId,
    condition: Expr,
    span: Span,
    specificity: u32,
    override_rank: u8,
}

impl CompiledRule {
    /// Creates a checked rule.
    #[must_use]
    pub fn new(
        id: RuleId,
        qualified_id: QualifiedRuleId,
        condition: Expr,
        span: Span,
        specificity: u32,
    ) -> Self {
        Self {
            id,
            qualified_id,
            condition,
            span,
            specificity,
            override_rank: 0,
        }
    }

    /// Creates a checked rule with a statically derived explicit-override rank.
    #[must_use]
    pub fn new_with_override(
        id: RuleId,
        qualified_id: QualifiedRuleId,
        condition: Expr,
        span: Span,
        specificity: u32,
        explicit_override: bool,
    ) -> Self {
        Self {
            id,
            qualified_id,
            condition,
            span,
            specificity,
            override_rank: u8::from(explicit_override),
        }
    }

    /// Returns the rule identifier.
    #[must_use]
    pub fn id(&self) -> &RuleId {
        &self.id
    }

    /// Returns the globally qualified rule identity.
    #[must_use]
    pub fn qualified_id(&self) -> &QualifiedRuleId {
        &self.qualified_id
    }

    /// Returns the checked condition expression.
    #[must_use]
    pub fn condition(&self) -> &Expr {
        &self.condition
    }

    /// Returns the authored rule span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Returns `1` for an authored explicit override and `0` otherwise.
    #[must_use]
    pub const fn override_rank(&self) -> u8 {
        self.override_rank
    }
}

/// One checked compiled decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledDecision {
    id: DecisionId,
    rules: BTreeMap<RuleId, CompiledRule>,
}

impl CompiledDecision {
    /// Creates a checked compiled decision with unique rules.
    ///
    /// # Errors
    ///
    /// Returns [`PackageBuildError`] when the same rule id appears more than once.
    pub fn new(id: DecisionId, rules: Vec<CompiledRule>) -> Result<Self, PackageBuildError> {
        let mut map = BTreeMap::new();
        for rule in rules {
            let key = rule.id.clone();
            if map.insert(key.clone(), rule).is_some() {
                return Err(PackageBuildError::DuplicateRule {
                    decision: id,
                    rule: key,
                });
            }
        }
        Ok(Self { id, rules: map })
    }

    /// Returns the decision identifier.
    #[must_use]
    pub fn id(&self) -> &DecisionId {
        &self.id
    }

    /// Returns checked rules keyed by id.
    #[must_use]
    pub fn rules(&self) -> &BTreeMap<RuleId, CompiledRule> {
        &self.rules
    }
}

/// Hash inputs retained for compiler reproducibility.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub struct CompilationInput {
    source_bundle_hash: ContentHash,
    vocabulary_hash: ContentHash,
    lock_hash: Option<ContentHash>,
}

impl CompilationInput {
    /// Creates a compilation input descriptor.
    #[must_use]
    pub const fn new(
        source_bundle_hash: ContentHash,
        vocabulary_hash: ContentHash,
        lock_hash: Option<ContentHash>,
    ) -> Self {
        Self {
            source_bundle_hash,
            vocabulary_hash,
            lock_hash,
        }
    }
}

/// Integrity material retained by the compiled package payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIntegritySet {
    input: CompilationInput,
}

impl PackageIntegritySet {
    /// Creates an integrity set from compilation input hashes.
    #[must_use]
    pub const fn new(input: CompilationInput) -> Self {
        Self { input }
    }
}

/// Wire payload for `rulery.compiled-package/v1`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledPackageV1 {
    package_id: PackageId,
    package_version: Version,
    language_version: LanguageVersion,
    compiler_identity: String,
    decisions: BTreeMap<DecisionId, CompiledDecision>,
    actions: BTreeMap<ActionId, CompiledAction>,
    vocabulary: ResolvedVocabulary,
    source_map: SourceMap,
    integrity: PackageIntegritySet,
    package_hash: ContentHash,
}

impl CompiledPackageV1 {
    /// Returns the package id.
    #[must_use]
    pub fn package_id(&self) -> &PackageId {
        &self.package_id
    }

    /// Returns the package version.
    #[must_use]
    pub fn package_version(&self) -> &Version {
        &self.package_version
    }

    /// Returns the language version.
    #[must_use]
    pub const fn language_version(&self) -> LanguageVersion {
        self.language_version
    }

    /// Returns the compiler identity.
    #[must_use]
    pub fn compiler_identity(&self) -> &str {
        &self.compiler_identity
    }

    /// Returns compiled decisions keyed by id.
    #[must_use]
    pub fn decisions(&self) -> &BTreeMap<DecisionId, CompiledDecision> {
        &self.decisions
    }

    /// Returns compiled actions keyed by id.
    #[must_use]
    pub fn actions(&self) -> &BTreeMap<ActionId, CompiledAction> {
        &self.actions
    }

    /// Returns the resolved vocabulary retained for evaluation.
    #[must_use]
    pub fn vocabulary(&self) -> &ResolvedVocabulary {
        &self.vocabulary
    }

    /// Returns the source map retained for provenance.
    #[must_use]
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    /// Returns retained compilation integrity material.
    #[must_use]
    pub const fn integrity(&self) -> &PackageIntegritySet {
        &self.integrity
    }

    /// Returns the content hash for this payload.
    #[must_use]
    pub const fn package_hash(&self) -> ContentHash {
        self.package_hash
    }
}

/// Builder input for a validated compiled package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledPackageDraft {
    /// Package identity.
    pub package_id: PackageId,
    /// Package semantic version.
    pub package_version: Version,
    /// Target language version.
    pub language_version: LanguageVersion,
    /// Compiler identity string.
    pub compiler_identity: String,
    /// Candidate decisions.
    pub decisions: Vec<CompiledDecision>,
    /// Candidate actions.
    pub actions: Vec<CompiledAction>,
    /// Input and integrity hashes.
    pub integrity: PackageIntegritySet,
}

/// Validated compiled package with retained non-wire structures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledPackage {
    payload: CompiledPackageV1,
}

impl CompiledPackage {
    /// Creates a validated compiled package and computes its content hash.
    ///
    /// # Errors
    ///
    /// Returns [`PackageBuildError`] when identity maps contain duplicates or required metadata is
    /// missing.
    pub fn new(
        draft: CompiledPackageDraft,
        source_map: SourceMap,
        vocabulary: ResolvedVocabulary,
    ) -> Result<Self, PackageBuildError> {
        let payload = build_payload(draft, source_map, vocabulary)?;
        Ok(Self { payload })
    }

    /// Revalidates a wire payload and rejects mismatched content hashes.
    ///
    /// # Errors
    ///
    /// Returns [`PackageBuildError::MismatchedContentHash`] when the embedded hash does not match
    /// the canonical payload hash.
    pub fn from_payload(
        payload: CompiledPackageV1,
        source_map: &SourceMap,
        vocabulary: &ResolvedVocabulary,
    ) -> Result<Self, PackageBuildError> {
        let expected = payload.package_hash;
        let computed = compute_payload_hash(&payload)?;
        if expected != computed {
            return Err(PackageBuildError::MismatchedContentHash { expected, computed });
        }
        if &payload.source_map != source_map || &payload.vocabulary != vocabulary {
            return Err(PackageBuildError::RetainedDataMismatch);
        }
        Ok(Self { payload })
    }

    /// Returns the validated payload.
    #[must_use]
    pub fn payload(&self) -> &CompiledPackageV1 {
        &self.payload
    }

    /// Returns the retained source map.
    #[must_use]
    pub fn source_map(&self) -> &SourceMap {
        &self.payload.source_map
    }

    /// Returns the retained resolved vocabulary.
    #[must_use]
    pub fn vocabulary(&self) -> &ResolvedVocabulary {
        &self.payload.vocabulary
    }

    /// Returns whether the package declares a decision identity.
    #[must_use]
    pub fn contains_decision(&self, decision: &DecisionId) -> bool {
        self.payload.decisions.contains_key(decision)
    }

    /// Returns whether the package contains a globally qualified rule.
    #[must_use]
    pub fn contains_rule(&self, rule: &QualifiedRuleId) -> bool {
        self.payload.decisions.values().any(|decision| {
            decision
                .rules
                .values()
                .any(|candidate| &candidate.qualified_id == rule)
        })
    }

    /// Returns whether retained vocabulary declares the complete fact path.
    #[must_use]
    pub fn contains_fact_path(&self, path: &rulery_contracts::FactPath) -> bool {
        let Some((root_path, root)) = self
            .payload
            .vocabulary
            .roots
            .iter()
            .find(|(root_path, _)| path.segments().starts_with(root_path.segments()))
        else {
            return false;
        };
        let mut offset = root_path.len();
        let mut type_id = &root.type_id;
        while offset < path.len() {
            let Some(resolved) = self.payload.vocabulary.types.get(type_id) else {
                return false;
            };
            match &resolved.declaration {
                TypeDeclaration::Alias { target, .. } => type_id = target,
                TypeDeclaration::Record { fields, .. } => {
                    let segment = &path.segments()[offset];
                    let Some(field) = fields.iter().find_map(|(name, field)| {
                        (name.as_str() == segment.as_str()).then_some(field)
                    }) else {
                        return false;
                    };
                    type_id = &field.type_id;
                    offset += 1;
                }
                TypeDeclaration::Primitive
                | TypeDeclaration::Enum { .. }
                | TypeDeclaration::List { .. } => return false,
            }
        }
        true
    }

    /// Returns deterministic JSON bytes for the complete v1 payload.
    ///
    /// # Errors
    ///
    /// Returns [`PackageBuildError`] if payload serialization fails.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, PackageBuildError> {
        serde_json::to_vec(&self.payload).map_err(PackageBuildError::HashSerialization)
    }

    /// Returns deterministic hash input bytes with only `package_hash` omitted.
    ///
    /// # Errors
    ///
    /// Returns [`PackageBuildError`] if canonical payload serialization fails.
    pub fn hash_input_bytes(&self) -> Result<Vec<u8>, PackageBuildError> {
        serde_json::to_vec(&CanonicalPayload::from(&self.payload))
            .map_err(PackageBuildError::HashSerialization)
    }
}

fn build_payload(
    draft: CompiledPackageDraft,
    source_map: SourceMap,
    vocabulary: ResolvedVocabulary,
) -> Result<CompiledPackageV1, PackageBuildError> {
    if draft.compiler_identity.trim().is_empty() {
        return Err(PackageBuildError::EmptyCompilerIdentity);
    }

    let mut decisions = BTreeMap::new();
    for decision in draft.decisions {
        let key = decision.id.clone();
        if decisions.insert(key.clone(), decision).is_some() {
            return Err(PackageBuildError::DuplicateDecision { decision: key });
        }
    }

    let mut actions = BTreeMap::new();
    for action in draft.actions {
        let key = action.id.clone();
        if actions.insert(key.clone(), action).is_some() {
            return Err(PackageBuildError::DuplicateAction { action: key });
        }
    }

    let mut payload = CompiledPackageV1 {
        package_id: draft.package_id,
        package_version: draft.package_version,
        language_version: draft.language_version,
        compiler_identity: draft.compiler_identity,
        decisions,
        actions,
        vocabulary,
        source_map,
        integrity: draft.integrity,
        package_hash: ContentHash::digest(&[]),
    };
    payload.package_hash = compute_payload_hash(&payload)?;
    Ok(payload)
}

fn compute_payload_hash(payload: &CompiledPackageV1) -> Result<ContentHash, PackageBuildError> {
    // The v1 hash contract is the serde representation of CanonicalPayload. BTreeMap ordering and
    // field order are therefore deliberate, and package_hash is omitted to avoid self-reference.
    let canonical = CanonicalPayload::from(payload);
    let bytes = serde_json::to_vec(&canonical).map_err(PackageBuildError::HashSerialization)?;
    Ok(hash_parts(
        HashDomain::CompiledPackageV1,
        [bytes.as_slice()],
    ))
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CanonicalPayload<'a> {
    package_id: &'a PackageId,
    package_version: &'a Version,
    language_version: LanguageVersion,
    compiler_identity: &'a str,
    decisions: &'a BTreeMap<DecisionId, CompiledDecision>,
    actions: &'a BTreeMap<ActionId, CompiledAction>,
    vocabulary: &'a ResolvedVocabulary,
    source_map: &'a SourceMap,
    integrity: &'a PackageIntegritySet,
}

impl<'a> From<&'a CompiledPackageV1> for CanonicalPayload<'a> {
    fn from(value: &'a CompiledPackageV1) -> Self {
        Self {
            package_id: &value.package_id,
            package_version: &value.package_version,
            language_version: value.language_version,
            compiler_identity: &value.compiler_identity,
            decisions: &value.decisions,
            actions: &value.actions,
            vocabulary: &value.vocabulary,
            source_map: &value.source_map,
            integrity: &value.integrity,
        }
    }
}

/// Error returned when building or validating compiled package invariants.
#[derive(Debug, Error)]
pub enum PackageBuildError {
    /// Two decisions use the same identity.
    #[error("compiled package cannot contain duplicate decision `{decision}`")]
    DuplicateDecision {
        /// Repeated decision identity.
        decision: DecisionId,
    },
    /// Two rules inside one decision use the same identity.
    #[error("decision `{decision}` cannot contain duplicate rule `{rule}`")]
    DuplicateRule {
        /// Decision identity.
        decision: DecisionId,
        /// Repeated rule identity.
        rule: RuleId,
    },
    /// Two actions use the same identity.
    #[error("compiled package cannot contain duplicate action `{action}`")]
    DuplicateAction {
        /// Repeated action identity.
        action: ActionId,
    },
    /// Compiler identity was empty.
    #[error("compiler identity must not be empty")]
    EmptyCompilerIdentity,
    /// The embedded package hash did not match the computed canonical hash.
    #[error("compiled package hash mismatch: expected {expected}, computed {computed}")]
    MismatchedContentHash {
        /// Hash read from payload.
        expected: ContentHash,
        /// Hash recomputed from canonical payload bytes.
        computed: ContentHash,
    },
    /// Canonical hash serialization failed.
    #[error("failed to serialize payload for content hashing: {0}")]
    HashSerialization(serde_json::Error),
    /// Wire payload and separately supplied retained data disagree.
    #[error("compiled package retained source map or vocabulary mismatch")]
    RetainedDataMismatch,
}

impl PartialEq for PackageBuildError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::DuplicateDecision { decision: left },
                Self::DuplicateDecision { decision: right },
            ) => left == right,
            (
                Self::DuplicateRule {
                    decision: left_decision,
                    rule: left_rule,
                },
                Self::DuplicateRule {
                    decision: right_decision,
                    rule: right_rule,
                },
            ) => left_decision == right_decision && left_rule == right_rule,
            (Self::DuplicateAction { action: left }, Self::DuplicateAction { action: right }) => {
                left == right
            }
            (Self::EmptyCompilerIdentity, Self::EmptyCompilerIdentity)
            | (Self::HashSerialization(_), Self::HashSerialization(_))
            | (Self::RetainedDataMismatch, Self::RetainedDataMismatch) => true,
            (
                Self::MismatchedContentHash {
                    expected: left_expected,
                    computed: left_computed,
                },
                Self::MismatchedContentHash {
                    expected: right_expected,
                    computed: right_computed,
                },
            ) => left_expected == right_expected && left_computed == right_computed,
            _ => false,
        }
    }
}

impl Eq for PackageBuildError {}
