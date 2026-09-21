//! Structural case-fact validation against resolved vocabulary.

use std::collections::BTreeMap;
use std::str::FromStr;

use rulery_contracts::{CaseFacts, FactRootId, FactValidationError, TypeId, Value, ValueKind};

use crate::model::{FieldPresence, ResolvedVocabulary, TypeDeclaration};

/// Lookup state for a resolved fact path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactLookupState<'a> {
    /// Path has no supplied value.
    Absent,
    /// Path has explicit null.
    Null,
    /// Path has a valid typed value.
    Valid(&'a Value),
    /// Path has malformed supplied value.
    Malformed(FactValidationError),
}

/// Additional non-shape validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationFailure {
    /// Required field was absent.
    RequiredMissing(String),
}

/// Validation result retaining states and failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedFacts<'a> {
    /// Deterministic path states.
    pub states: BTreeMap<String, FactLookupState<'a>>,
    /// Deterministic validation failures.
    pub failures: Vec<ValidationFailure>,
}

/// Validates supplied facts against resolved declarations.
#[must_use]
pub fn validate_case_facts<'a>(
    vocabulary: &'a ResolvedVocabulary,
    facts: &'a CaseFacts,
) -> ValidatedFacts<'a> {
    let mut states = BTreeMap::new();
    let mut failures = Vec::new();

    for root in vocabulary.roots.values() {
        let Some(first_segment) = root.path.segments().first() else {
            continue;
        };
        let Ok(root_id) = FactRootId::new(first_segment.as_str()) else {
            continue;
        };
        let root_state = facts.root(&root_id);
        let root_key = root.path.to_string();

        match root_state {
            rulery_contracts::FactState::Absent => {
                states.insert(root_key, FactLookupState::Absent);
            }
            rulery_contracts::FactState::Null => {
                states.insert(root_key, FactLookupState::Null);
            }
            rulery_contracts::FactState::Malformed(error) => {
                states.insert(root_key, FactLookupState::Malformed(error));
            }
            rulery_contracts::FactState::Valid(value) => {
                states.insert(root_key.clone(), FactLookupState::Valid(value));
                validate_value(
                    &mut states,
                    &mut failures,
                    &root_key,
                    value,
                    &root.type_id,
                    vocabulary,
                );
            }
        }
    }

    ValidatedFacts { states, failures }
}

#[allow(clippy::too_many_lines)]
fn validate_value<'a>(
    states: &mut BTreeMap<String, FactLookupState<'a>>,
    failures: &mut Vec<ValidationFailure>,
    path: &str,
    value: &'a Value,
    expected: &TypeId,
    vocabulary: &'a ResolvedVocabulary,
) {
    let Some(resolved_type) = vocabulary.types.get(expected) else {
        states.insert(
            path.to_owned(),
            FactLookupState::Malformed(FactValidationError::TypeMismatch {
                expected: expected.clone(),
                actual: kind_of(value),
            }),
        );
        return;
    };

    match (&resolved_type.declaration, value) {
        (TypeDeclaration::Record { fields, closed, .. }, Value::Record(record)) => {
            if *closed {
                for unknown in record.keys().filter(|name| !fields.contains_key(*name)) {
                    states.insert(
                        format!("{path}.{}", unknown.as_str()),
                        FactLookupState::Malformed(FactValidationError::UnknownRecordField {
                            field: unknown.clone(),
                        }),
                    );
                }
            }

            for (field_name, declaration) in fields {
                let child_path = format!("{path}.{}", field_name.as_str());
                match record.get(field_name) {
                    None => {
                        states.insert(child_path.clone(), FactLookupState::Absent);
                        if declaration.presence == FieldPresence::Required {
                            failures.push(ValidationFailure::RequiredMissing(child_path));
                        }
                    }
                    Some(_) if declaration.derived => {
                        let derived_path = rulery_contracts::FactPath::from_str(&child_path)
                            .unwrap_or_else(|_| {
                                rulery_contracts::FactPath::from_str("derived").expect("fact path")
                            });
                        states.insert(
                            child_path,
                            FactLookupState::Malformed(FactValidationError::DerivedFieldSupplied {
                                path: derived_path,
                            }),
                        );
                    }
                    Some(Value::Null) => {
                        states.insert(child_path, FactLookupState::Null);
                    }
                    Some(child) => {
                        states.insert(child_path.clone(), FactLookupState::Valid(child));
                        validate_value(
                            states,
                            failures,
                            &child_path,
                            child,
                            &declaration.type_id,
                            vocabulary,
                        );
                    }
                }
            }
        }
        (TypeDeclaration::Enum { .. }, Value::Enum(_)) => {
            states.insert(
                path.to_owned(),
                FactLookupState::Malformed(FactValidationError::OutOfRange),
            );
        }
        (
            TypeDeclaration::List {
                min_items,
                max_items,
                ..
            },
            Value::List(values),
        ) => {
            if min_items.is_some_and(|minimum| {
                u64::try_from(values.len())
                    .ok()
                    .is_none_or(|count| count < minimum)
            }) || max_items.is_some_and(|maximum| {
                u64::try_from(values.len())
                    .ok()
                    .is_none_or(|count| count > maximum)
            }) {
                states.insert(
                    path.to_owned(),
                    FactLookupState::Malformed(FactValidationError::OutOfRange),
                );
            }
        }
        (TypeDeclaration::Alias { target, .. }, _) => {
            validate_value(states, failures, path, value, target, vocabulary);
        }
        (
            TypeDeclaration::Primitive
            | TypeDeclaration::Enum { .. }
            | TypeDeclaration::List { .. }
            | TypeDeclaration::Record { .. },
            _,
        ) => {
            states.insert(
                path.to_owned(),
                FactLookupState::Malformed(FactValidationError::TypeMismatch {
                    expected: expected.clone(),
                    actual: kind_of(value),
                }),
            );
        }
    }
}

fn kind_of(value: &Value) -> ValueKind {
    match value {
        Value::Null => ValueKind::Null,
        Value::Boolean(_) => ValueKind::Boolean,
        Value::Integer(_) => ValueKind::Integer,
        Value::Decimal(_) => ValueKind::Decimal,
        Value::Text(_) => ValueKind::Text,
        Value::Date(_) => ValueKind::Date,
        Value::DateTime(_) => ValueKind::DateTime,
        Value::Duration(_) => ValueKind::Duration,
        Value::Enum(_) => ValueKind::Enum,
        Value::List(_) => ValueKind::List,
        Value::Record(_) => ValueKind::Record,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use rulery_contracts::{CaseFacts, FactPath, FactRootId, StableId, TypeId, Value};

    use crate::model::{
        EnumVariant, FieldDeclaration, FieldPresence, ResolvedRoot, ResolvedType,
        ResolvedVocabulary, TypeDeclaration,
    };

    use super::*;

    #[test]
    fn fact_validation_preserves_absent_null_and_malformed() {
        let account_type = TypeId::new("type.account").expect("type");
        let status_type = TypeId::new("type.status").expect("type");
        let optional_type = TypeId::new("type.optional").expect("type");

        let vocabulary = ResolvedVocabulary {
            roots: BTreeMap::from([(
                FactPath::from_str("account").expect("path"),
                ResolvedRoot {
                    path: FactPath::from_str("account").expect("path"),
                    type_id: account_type.clone(),
                },
            )]),
            types: BTreeMap::from([
                (
                    account_type.clone(),
                    ResolvedType {
                        id: account_type.clone(),
                        declaration: TypeDeclaration::Record {
                            id: account_type.clone(),
                            fields: BTreeMap::from([
                                (
                                    StableId::new("status").expect("field"),
                                    FieldDeclaration {
                                        type_id: status_type.clone(),
                                        presence: FieldPresence::Required,
                                        derived: false,
                                    },
                                ),
                                (
                                    StableId::new("optional").expect("field"),
                                    FieldDeclaration {
                                        type_id: optional_type.clone(),
                                        presence: FieldPresence::Optional,
                                        derived: false,
                                    },
                                ),
                                (
                                    StableId::new("derived").expect("field"),
                                    FieldDeclaration {
                                        type_id: optional_type.clone(),
                                        presence: FieldPresence::Optional,
                                        derived: true,
                                    },
                                ),
                            ]),
                            closed: true,
                        },
                    },
                ),
                (
                    status_type.clone(),
                    ResolvedType {
                        id: status_type.clone(),
                        declaration: TypeDeclaration::Enum {
                            id: status_type.clone(),
                            variants: vec![EnumVariant {
                                symbol: StableId::new("status.active").expect("variant"),
                            }],
                        },
                    },
                ),
                (
                    optional_type.clone(),
                    ResolvedType {
                        id: optional_type.clone(),
                        declaration: TypeDeclaration::Primitive,
                    },
                ),
            ]),
            terms: BTreeMap::new(),
        };

        let absent = CaseFacts::default();
        let absent_validated = validate_case_facts(&vocabulary, &absent);
        assert!(matches!(
            absent_validated.states.get("account"),
            Some(FactLookupState::Absent)
        ));

        let null_case = CaseFacts::new(BTreeMap::from([(
            FactRootId::new("account").expect("root"),
            Value::Null,
        )]));
        let null_validated = validate_case_facts(&vocabulary, &null_case);
        assert!(matches!(
            null_validated.states.get("account"),
            Some(FactLookupState::Null)
        ));

        let malformed_case = CaseFacts::new(BTreeMap::from([(
            FactRootId::new("account").expect("root"),
            Value::Integer(42),
        )]));
        let malformed_validated = validate_case_facts(&vocabulary, &malformed_case);
        assert!(matches!(
            malformed_validated.states.get("account"),
            Some(FactLookupState::Malformed(_))
        ));

        let valid_case = CaseFacts::new(BTreeMap::from([(
            FactRootId::new("account").expect("root"),
            Value::Record(BTreeMap::from([
                (
                    StableId::new("status").expect("field"),
                    Value::Text("bad".to_owned()),
                ),
                (
                    StableId::new("extra").expect("field"),
                    Value::Text("x".to_owned()),
                ),
                (
                    StableId::new("derived").expect("field"),
                    Value::Text("computed".to_owned()),
                ),
            ])),
        )]));
        let validated = validate_case_facts(&vocabulary, &valid_case);

        assert!(matches!(
            validated.states.get("account.optional"),
            Some(FactLookupState::Absent)
        ));
        assert!(
            validated
                .states
                .get("account.status")
                .is_some_and(|state| matches!(state, FactLookupState::Malformed(_)))
        );
        assert!(
            validated
                .states
                .get("account.extra")
                .is_some_and(|state| matches!(state, FactLookupState::Malformed(_)))
        );
        assert!(
            validated
                .states
                .get("account.derived")
                .is_some_and(|state| matches!(state, FactLookupState::Malformed(_)))
        );
        let missing_required_case = CaseFacts::new(BTreeMap::from([(
            FactRootId::new("account").expect("root"),
            Value::Record(BTreeMap::new()),
        )]));
        let missing_required = validate_case_facts(&vocabulary, &missing_required_case);
        assert!(missing_required
            .failures
            .iter()
            .any(|failure| matches!(failure, ValidationFailure::RequiredMissing(path) if path == "account.status")));
        assert!(matches!(
            missing_required.states.get("account.status"),
            Some(FactLookupState::Absent)
        ));
    }
}
