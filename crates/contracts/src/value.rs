//! Typed policy values and supplied facts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    DecimalValue, DurationValue, FactPath, FactRootId, PolicyDate, StableId, TypeId, UtcInstant,
};

/// Runtime kind of a typed policy value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    /// Explicit null.
    Null,
    /// Boolean value.
    Boolean,
    /// Signed integer value.
    Integer,
    /// Exact decimal value.
    Decimal,
    /// Unicode text value.
    Text,
    /// Civil policy date.
    Date,
    /// UTC instant.
    DateTime,
    /// Exact duration.
    Duration,
    /// Typed enumeration value.
    Enum,
    /// Ordered list value.
    List,
    /// Record value.
    Record,
}

/// Typed policy value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Value {
    /// Explicit null.
    Null,
    /// Boolean value.
    Boolean(bool),
    /// Signed integer value.
    Integer(i64),
    /// Exact decimal value.
    Decimal(DecimalValue),
    /// Unicode text value.
    Text(String),
    /// Civil policy date.
    Date(PolicyDate),
    /// UTC instant.
    DateTime(UtcInstant),
    /// Exact duration.
    Duration(DurationValue),
    /// Typed enumeration value.
    Enum(EnumValue),
    /// Ordered list value.
    List(Vec<Self>),
    /// Record value with deterministic field ordering.
    Record(BTreeMap<StableId, Self>),
}

/// Enumeration value retaining its declared type identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumValue {
    type_id: TypeId,
    variant: StableId,
}

impl EnumValue {
    /// Creates a typed enumeration value.
    #[must_use]
    pub const fn new(type_id: TypeId, variant: StableId) -> Self {
        Self { type_id, variant }
    }

    /// Returns the declared enum type.
    #[must_use]
    pub const fn type_id(&self) -> &TypeId {
        &self.type_id
    }

    /// Returns the selected variant.
    #[must_use]
    pub const fn variant(&self) -> &StableId {
        &self.variant
    }
}

/// Supplied root facts.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaseFacts {
    roots: BTreeMap<FactRootId, Value>,
}

impl CaseFacts {
    /// Creates a fact set from typed root values.
    #[must_use]
    pub fn new(roots: BTreeMap<FactRootId, Value>) -> Self {
        Self { roots }
    }

    /// Returns the supplied state of a root fact.
    #[must_use]
    pub fn root(&self, root: &FactRootId) -> FactState<'_> {
        match self.roots.get(root) {
            None => FactState::Absent,
            Some(Value::Null) => FactState::Null,
            Some(value) => FactState::Valid(value),
        }
    }
}

/// State observed while resolving one fact path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactState<'a> {
    /// No value was supplied.
    Absent,
    /// Explicit null was supplied.
    Null,
    /// A valid typed value was supplied.
    Valid(&'a Value),
    /// Supplied evidence violates its declared contract.
    Malformed(FactValidationError),
}

/// Structural fact validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FactValidationError {
    /// Value kind differs from the declared type.
    #[error("expected `{expected}`, found {actual:?}")]
    TypeMismatch {
        /// Expected vocabulary type.
        expected: TypeId,
        /// Actual runtime value kind.
        actual: ValueKind,
    },
    /// Enumeration variant is absent from its declaration.
    #[error("invalid variant `{variant}` for enum `{type_id}`")]
    InvalidEnumVariant {
        /// Enumeration type.
        type_id: TypeId,
        /// Rejected variant.
        variant: StableId,
    },
    /// Numeric or collection value exceeds its declared range.
    #[error("value is outside its declared range")]
    OutOfRange,
    /// Record contains an undeclared field.
    #[error("unknown record field `{field}`")]
    UnknownRecordField {
        /// Rejected record field.
        field: StableId,
    },
    /// Caller supplied a derived field.
    #[error("derived fact `{path}` cannot be supplied")]
    DerivedFieldSupplied {
        /// Derived fact path.
        path: FactPath,
    },
}
