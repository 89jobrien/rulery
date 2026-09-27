//! Canonical authored fact paths to root-keyed case facts.
//!
//! [`CaseFacts`](rulery_contracts::CaseFacts) is keyed by root identity, while authored rules,
//! scenarios, and partition cells all name facts by canonical path. Nesting is therefore a required
//! conversion, and it must have exactly one implementation: a witness that nests differently from
//! the scenario it claims to reproduce would not replay.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

use rulery_contracts::{CaseFacts, FactPath, FactRootId, FactSegment, StableId, Value};

/// Authored fact paths that cannot be nested into one root-keyed value map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaseFactsError {
    /// A path carried no segments.
    EmptyPath(String),
    /// A path segment was rejected as an identifier.
    InvalidSegment {
        /// Offending authored path.
        path: String,
        /// Segment rejection reason.
        reason: String,
    },
    /// Two authored paths cannot occupy the same position in one value map.
    ///
    /// Ascending key order places a single-segment root before every path nested under it, so a
    /// root value is always inserted first and the collision is always reported against the child
    /// that could not occupy it.
    PathConflict(String),
    /// A child path collides with a non-record value.
    NonRecordField {
        /// Offending authored path.
        path: String,
        /// Field that already holds a scalar.
        field: String,
    },
}

impl fmt::Display for CaseFactsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath(path) => write!(formatter, "fact path `{path}` has no segments"),
            Self::InvalidSegment { path, reason } => {
                write!(formatter, "fact path `{path}`: {reason}")
            }
            Self::PathConflict(path) => write!(
                formatter,
                "fact path `{path}` conflicts with an authored path at the same position"
            ),
            Self::NonRecordField { path, field } => write!(
                formatter,
                "fact path `{path}`: field `{field}` is already a non-record value"
            ),
        }
    }
}

impl std::error::Error for CaseFactsError {}

/// Nests one canonical path-keyed assignment map into root-keyed [`CaseFacts`].
///
/// A single-segment path supplies that root's value directly, and a deeper path becomes nested
/// records. Absence is represented by omitting the key, so an assignment map is expected to contain
/// only supplied evidence.
///
/// # Errors
///
/// Returns [`CaseFactsError`] when a path is empty, a root identity is invalid, or two paths cannot
/// occupy the same position in one value map.
pub fn build_case_facts(
    assignments: &BTreeMap<FactPath, Value>,
) -> Result<CaseFacts, CaseFactsError> {
    let mut roots: BTreeMap<FactRootId, Value> = BTreeMap::new();
    for (path, value) in assignments {
        let first = path
            .segments()
            .first()
            .ok_or_else(|| CaseFactsError::EmptyPath(path.to_string()))?;
        let root_id =
            FactRootId::new(first.as_str()).map_err(|error| CaseFactsError::InvalidSegment {
                path: path.to_string(),
                reason: error.to_string(),
            })?;
        let rest = &path.segments()[1..];
        match roots.entry(root_id) {
            Entry::Vacant(slot) => {
                if rest.is_empty() {
                    slot.insert(value.clone());
                } else {
                    let mut fields = BTreeMap::new();
                    insert_record(&mut fields, rest, value, path)?;
                    slot.insert(Value::Record(fields));
                }
            }
            Entry::Occupied(mut slot) => {
                let Value::Record(fields) = slot.get_mut() else {
                    return Err(CaseFactsError::PathConflict(path.to_string()));
                };
                if rest.is_empty() {
                    // Two distinct paths cannot share a root and both be single-segment, so
                    // ascending key order rules this out; the error keeps the function total.
                    return Err(CaseFactsError::PathConflict(path.to_string()));
                }
                insert_record(fields, rest, value, path)?;
            }
        }
    }
    Ok(CaseFacts::new(roots))
}

fn insert_record(
    fields: &mut BTreeMap<StableId, Value>,
    segments: &[FactSegment],
    value: &Value,
    path: &FactPath,
) -> Result<(), CaseFactsError> {
    let Some((first, rest)) = segments.split_first() else {
        return Err(CaseFactsError::EmptyPath(path.to_string()));
    };
    let first = StableId::new(first.as_str()).map_err(|error| CaseFactsError::InvalidSegment {
        path: path.to_string(),
        reason: error.to_string(),
    })?;
    if rest.is_empty() {
        fields.insert(first, value.clone());
        return Ok(());
    }
    let entry = fields
        .entry(first.clone())
        .or_insert_with(|| Value::Record(BTreeMap::new()));
    match entry {
        Value::Record(nested) => insert_record(nested, rest, value, path),
        _ => Err(CaseFactsError::NonRecordField {
            path: path.to_string(),
            field: first.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rulery_contracts::{FactRootId, Value};

    use super::*;

    #[test]
    fn case_facts_nest_authored_paths_and_reject_conflicts() {
        let path = |value: &str| FactPath::from_str(value).expect("path");
        let facts = build_case_facts(&BTreeMap::from([
            (path("member.id"), Value::Text("m-1".to_owned())),
            (path("member.age"), Value::Integer(41)),
            (path("active"), Value::Boolean(true)),
        ]))
        .expect("root facts");

        let member = facts.root(&FactRootId::new("member").expect("root id"));
        let rulery_contracts::FactState::Valid(Value::Record(fields)) = member else {
            panic!("member root must be a record");
        };
        assert_eq!(fields.len(), 2);
        assert!(matches!(
            fields.get(&StableId::new("id").expect("id")),
            Some(Value::Text(value)) if value == "m-1"
        ));
        assert!(matches!(
            fields.get(&StableId::new("age").expect("age")),
            Some(Value::Integer(41))
        ));
        assert!(matches!(
            facts.root(&FactRootId::new("active").expect("root id")),
            rulery_contracts::FactState::Valid(Value::Boolean(true))
        ));

        assert_eq!(
            build_case_facts(&BTreeMap::from([
                (path("member"), Value::Text("root wins".to_owned())),
                (path("member.id"), Value::Text("child loses".to_owned())),
            ])),
            Err(CaseFactsError::PathConflict("member.id".to_owned()))
        );
        assert_eq!(
            build_case_facts(&BTreeMap::from([
                (path("member.id"), Value::Text("scalar".to_owned())),
                (path("member.id.deeper"), Value::Integer(1)),
            ])),
            Err(CaseFactsError::NonRecordField {
                path: "member.id.deeper".to_owned(),
                field: "id".to_owned(),
            })
        );
    }
}
