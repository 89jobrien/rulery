//! Format-neutral authored source declarations.
#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{
    DecisionId, FactPath, PackageId, QualifiedRuleId, SourceBundle, SourceFile, SourceId,
    SourceKey, SourceMap, SourcePath, Span, StableId, Version, VersionRequirement,
};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePackage {
    pub metadata: SourceMetadata,
    pub semantics: SourceSemantics,
    pub imports: Vec<SourceImport>,
    pub decisions: Vec<SourceDecision>,
    pub vocabulary: SourceVocabulary,
    pub actions: Vec<SourceAction>,
    pub scenario: Option<SourceScenario>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMetadata {
    pub package_id: PackageId,
    pub display_name: String,
    pub version: Version,
    pub language_version: u16,
    pub description: Option<String>,
    pub authors: Vec<SourceAuthor>,
    pub tags: BTreeSet<StableId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceAuthor {
    pub name: String,
    pub contact: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSemantics {
    pub timezone: String,
    pub expiry: String,
    pub missing_facts: SourceStrategy,
    pub invalid_facts: SourceStrategy,
    pub precedence: SourcePrecedence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceStrategy {
    pub kind: String,
    pub destination: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePrecedence {
    pub kind: String,
    pub primary: Option<String>,
    pub outcome_ranks: BTreeMap<String, u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceImport {
    pub package: PackageId,
    pub version: VersionRequirement,
    pub alias: Option<StableId>,
    pub path: SourcePath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDecision {
    pub id: DecisionId,
    pub title: String,
    pub asks: String,
    pub input_roots: BTreeSet<StableId>,
    pub default: SourceOutcome,
    pub rules: Vec<SourceRule>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceVocabulary {
    pub roots: BTreeMap<StableId, SourceRoot>,
    pub types: BTreeMap<StableId, SourceType>,
    pub terms: BTreeMap<StableId, SourceTerm>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRoot {
    pub type_id: String,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceType {
    pub kind: String,
    pub variants: BTreeMap<StableId, SourceVariant>,
    pub closed: Option<bool>,
    pub fields: BTreeMap<StableId, SourceField>,
    pub items: Option<String>,
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceVariant {
    pub display_name: String,
    pub description: Option<String>,
    pub deprecated: bool,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceField {
    pub type_id: String,
    pub presence: String,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceTerm {
    pub display_name: String,
    pub definition: String,
    pub applies_to: BTreeSet<FactPath>,
    pub examples: Vec<String>,
    pub counterexamples: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRule {
    pub id: StableId,
    pub title: Option<String>,
    pub priority: i32,
    pub when: SourceCondition,
    pub effect: SourceOutcome,
    pub explicit_override: bool,
    pub rationale: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceCondition {
    All { conditions: Vec<Self>, span: Span },
    Any { conditions: Vec<Self>, span: Span },
    Not { condition: Box<Self>, span: Span },
    Predicate(SourcePredicate),
}

impl SourceCondition {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::All { span, .. } | Self::Any { span, .. } | Self::Not { span, .. } => *span,
            Self::Predicate(predicate) => predicate.span,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePredicate {
    pub fact: FactPath,
    pub operator: SourceOperator,
    pub value: Option<SourceOperand>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    IsOneOf,
    IsAbsent,
    IsPresent,
    IsValid,
    IsInvalid,
    Before,
    OnOrAfter,
    IsExpired,
    IsUnexpired,
}

/// Authored literal value preserving the structure the author wrote.
///
/// Scalars stay untyped text because the authored grammar requires the literals that need a
/// specific type to be quoted; the declared type at the point of use gives them their type. A
/// literal that has already been flattened to text cannot be decoded again, so sequences and
/// mappings keep their own shape here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceValue {
    /// Untyped scalar text, typed by the declared type where it is used.
    Scalar(String),
    /// Explicit null, distinct from the text `null`.
    Null,
    /// Ordered sequence literal.
    Sequence(Vec<SourceValue>),
    /// Key-sorted mapping literal keyed by stable field name.
    Mapping(BTreeMap<StableId, SourceValue>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceOperand {
    Literal(SourceValue),
    Fact(FactPath),
    Reserved(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceOutcome {
    pub kind: String,
    pub reasons: Vec<SourceReason>,
    pub actions: Vec<SourceActionInvocation>,
    pub destination: Option<String>,
    pub required_facts: BTreeSet<FactPath>,
    pub span: Span,
}

/// Backward-compatible name for a rule outcome.
pub type SourceEffect = SourceOutcome;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceReason {
    pub code: StableId,
    pub message: String,
    pub detail: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceActionInvocation {
    pub action: StableId,
    pub arguments: BTreeMap<StableId, SourceOperand>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceAction {
    pub id: StableId,
    pub display_name: String,
    pub description: Option<String>,
    pub parameters: BTreeMap<StableId, SourceActionParameter>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceActionParameter {
    pub type_id: String,
    pub required: bool,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceScenario {
    pub id: StableId,
    pub title: String,
    pub description: Option<String>,
    pub decision: DecisionId,
    pub at: String,
    pub given: BTreeMap<StableId, SourceOperand>,
    pub expect: SourceExpectedDecision,
    pub tags: BTreeSet<StableId>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceExpectedDecision {
    pub outcome: String,
    pub determining_rules: BTreeSet<QualifiedRuleId>,
    pub required_facts: BTreeSet<FactPath>,
    pub reason_codes: BTreeSet<StableId>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedPackage {
    pub package: SourcePackage,
    pub scenarios: Vec<SourceScenario>,
    pub source_map: SourceMap,
}

pub trait SourceParser {
    /// Parses one source input.
    ///
    /// # Errors
    ///
    /// Returns an error when the input is not a complete valid source package.
    fn parse(&self, input: &[u8]) -> Result<ParsedPackage, SourceParseError>;

    /// Parses the complete ordered authored source bundle.
    ///
    /// # Errors
    ///
    /// Returns an error when an authored document is invalid or required documents are absent.
    fn parse_bundle(&self, bundle: &SourceBundle) -> Result<ParsedPackage, SourceParseError> {
        let Some(manifest) = bundle
            .documents()
            .iter()
            .find(|document| document.path().as_str() == "rulery.yaml")
        else {
            return self.parse(&[]);
        };
        self.parse(manifest.content().as_bytes())
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{kind:?} parse error: {message}")]
pub struct SourceParseError {
    pub kind: SourceParseErrorKind,
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceParseErrorKind {
    Syntax,
    Shape,
    ResourceLimit,
}

pub(crate) fn map_for_bundle(bundle: &SourceBundle) -> SourceMap {
    let mut map = SourceMap::new();
    for (index, document) in bundle.documents().iter().enumerate() {
        let key = SourceKey::new(u32::try_from(index + 1).expect("bundle source key overflow"));
        map.insert(
            key,
            SourceFile::new(
                SourceId::new(format!("source.{}", key.get())).expect("generated id is valid"),
                document.path().clone(),
                std::sync::Arc::<str>::from(document.content()),
            ),
        )
        .expect("generated source keys are unique");
    }
    map
}
