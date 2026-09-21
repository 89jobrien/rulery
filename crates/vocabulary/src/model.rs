//! Resolved vocabulary model.

use std::collections::BTreeMap;

use rulery_contracts::{FactPath, StableId, TypeId};
use serde::{Deserialize, Serialize};

/// Raw vocabulary input declarations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VocabularyInput {
    /// Root path declarations.
    pub roots: Vec<ResolvedRoot>,
    /// Type declarations.
    pub types: Vec<TypeDeclaration>,
    /// Operational terms.
    pub terms: Vec<OperationalTerm>,
}

/// Resolved immutable vocabulary.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedVocabulary {
    /// Deterministic path-sorted roots.
    pub roots: BTreeMap<FactPath, ResolvedRoot>,
    /// Deterministic type-sorted declarations.
    pub types: BTreeMap<TypeId, ResolvedType>,
    /// Deterministic term-sorted declarations.
    pub terms: BTreeMap<StableId, OperationalTerm>,
}

/// One resolved root declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedRoot {
    /// Root fact path.
    pub path: FactPath,
    /// Type assigned to this root.
    pub type_id: TypeId,
}

/// One resolved type declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedType {
    /// Type identity.
    pub id: TypeId,
    /// Type structure.
    pub declaration: TypeDeclaration,
}

/// Declared type forms.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeDeclaration {
    /// Primitive built-in type.
    Primitive,
    /// Enum with declared variants.
    Enum {
        /// Type identity.
        id: TypeId,
        /// Allowed variants.
        variants: Vec<EnumVariant>,
    },
    /// Record with named fields.
    Record {
        /// Type identity.
        id: TypeId,
        /// Declared fields.
        fields: BTreeMap<StableId, FieldDeclaration>,
        /// Closed-world extra-field policy.
        closed: bool,
    },
    /// List of one element type.
    List {
        /// Type identity.
        id: TypeId,
        /// Element type identity.
        element: TypeId,
        /// Minimum item count.
        min_items: Option<u64>,
        /// Maximum item count.
        max_items: Option<u64>,
    },
    /// Alias to another type.
    Alias {
        /// Type identity.
        id: TypeId,
        /// Referenced target type.
        target: TypeId,
    },
}

/// One enum variant retaining symbol text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumVariant {
    /// Variant symbol.
    pub symbol: StableId,
}

/// Record field declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDeclaration {
    /// Field type identity.
    pub type_id: TypeId,
    /// Required/optional presence.
    pub presence: FieldPresence,
    /// Whether this field is derived-only.
    pub derived: bool,
}

/// Required/optional field presence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldPresence {
    /// Field must be present.
    Required,
    /// Field may be absent.
    Optional,
}

/// Operational vocabulary term declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalTerm {
    /// Term identity.
    pub id: StableId,
    /// Fact path this term applies to.
    pub applies_to: FactPath,
}
