//! Runtime fact lookup and predicate evaluation.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rulery_contracts::{FactPath, FactValidationError, Value};

use crate::Truth;

/// Supplied runtime evidence for one resolved fact path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactEvidence {
    /// Explicit null was supplied.
    Null,
    /// Supplied evidence violates its vocabulary contract.
    Malformed(FactValidationError),
    /// Supplied evidence is a valid typed value.
    Valid(Value),
}

/// Borrowed state returned by operand lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandState<'a> {
    /// No value was supplied for the path.
    Absent,
    /// Explicit null was supplied.
    Null,
    /// Evidence is malformed, retaining the validation failure.
    Malformed(&'a FactValidationError),
    /// Evidence is valid, retaining the typed value.
    Valid(&'a Value),
}

/// Fact lookup result retained for evaluation traces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperandLookup<'a> {
    /// Resolved path that was requested, including missing paths.
    pub path: FactPath,
    /// Runtime evidence state.
    pub state: OperandState<'a>,
}

/// Unary fact-state predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresencePredicate {
    /// True only when no evidence was supplied.
    IsAbsent,
    /// True whenever evidence exists, including null or malformed evidence.
    IsPresent,
    /// True only for valid typed evidence.
    IsValid,
    /// True only for malformed evidence.
    IsInvalid,
}

/// Binary typed predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryPredicate {
    /// Structural type-strict equality.
    Equals,
    /// Decisive negation of equality.
    NotEquals,
    /// Strict ordering.
    LessThan,
    /// Inclusive lower ordering.
    LessOrEqual,
    /// Strict reverse ordering.
    GreaterThan,
    /// Inclusive reverse ordering.
    GreaterOrEqual,
    /// Contiguous text or typed list-element containment.
    Contains,
    /// Decisive negation of containment.
    NotContains,
    /// Text prefix comparison.
    StartsWith,
    /// Text suffix comparison.
    EndsWith,
    /// Membership in a non-empty literal list.
    IsOneOf,
}

/// Looks up a fact path while preserving absence, null, malformed errors, and valid values.
#[must_use]
pub fn lookup_operand<'a>(
    facts: &'a BTreeMap<FactPath, FactEvidence>,
    path: &FactPath,
) -> OperandLookup<'a> {
    let state = match facts.get(path) {
        None => OperandState::Absent,
        Some(FactEvidence::Null) => OperandState::Null,
        Some(FactEvidence::Malformed(error)) => OperandState::Malformed(error),
        Some(FactEvidence::Valid(value)) => OperandState::Valid(value),
    };
    OperandLookup {
        path: path.clone(),
        state,
    }
}

/// Evaluates one unary presence/validity predicate.
#[must_use]
pub const fn evaluate_presence(predicate: PresencePredicate, state: OperandState<'_>) -> Truth {
    match predicate {
        PresencePredicate::IsAbsent => match state {
            OperandState::Absent => Truth::True,
            OperandState::Null | OperandState::Malformed(_) | OperandState::Valid(_) => {
                Truth::False
            }
        },
        PresencePredicate::IsPresent => match state {
            OperandState::Absent => Truth::False,
            OperandState::Null | OperandState::Malformed(_) | OperandState::Valid(_) => Truth::True,
        },
        PresencePredicate::IsValid => match state {
            OperandState::Absent => Truth::Unknown,
            OperandState::Null | OperandState::Malformed(_) => Truth::False,
            OperandState::Valid(_) => Truth::True,
        },
        PresencePredicate::IsInvalid => match state {
            OperandState::Absent => Truth::Unknown,
            OperandState::Null | OperandState::Valid(_) => Truth::False,
            OperandState::Malformed(_) => Truth::True,
        },
    }
}

/// Evaluates a structural, type-strict binary predicate.
#[must_use]
pub fn evaluate_binary(
    predicate: BinaryPredicate,
    left: OperandState<'_>,
    right: OperandState<'_>,
) -> Truth {
    if matches!(left, OperandState::Absent) || matches!(right, OperandState::Absent) {
        return Truth::Unknown;
    }
    if matches!(left, OperandState::Malformed(_)) || matches!(right, OperandState::Malformed(_)) {
        return Truth::Invalid;
    }

    match predicate {
        BinaryPredicate::Equals => equality(left, right),
        BinaryPredicate::NotEquals => equality(left, right).not(),
        BinaryPredicate::LessThan => ordering(left, right, OrderingTest::Less),
        BinaryPredicate::LessOrEqual => ordering(left, right, OrderingTest::LessOrEqual),
        BinaryPredicate::GreaterThan => ordering(left, right, OrderingTest::Greater),
        BinaryPredicate::GreaterOrEqual => ordering(left, right, OrderingTest::GreaterOrEqual),
        BinaryPredicate::Contains => contains(left, right),
        BinaryPredicate::NotContains => contains(left, right).not(),
        BinaryPredicate::StartsWith => {
            text_test(left, right, |text, prefix| text.starts_with(prefix))
        }
        BinaryPredicate::EndsWith => text_test(left, right, |text, suffix| text.ends_with(suffix)),
        BinaryPredicate::IsOneOf => is_one_of(left, right),
    }
}

fn equality(left: OperandState<'_>, right: OperandState<'_>) -> Truth {
    let left_null = matches!(left, OperandState::Null | OperandState::Valid(Value::Null));
    let right_null = matches!(right, OperandState::Null | OperandState::Valid(Value::Null));
    if left_null || right_null {
        return Truth::from(left_null && right_null);
    }
    match (left, right) {
        (OperandState::Valid(left), OperandState::Valid(right)) => Truth::from(left == right),
        _ => unreachable_state(),
    }
}

fn contains(left: OperandState<'_>, right: OperandState<'_>) -> Truth {
    match (left, right) {
        (OperandState::Valid(Value::Text(left)), OperandState::Valid(Value::Text(right))) => {
            Truth::from(left.contains(right))
        }
        (OperandState::Valid(Value::List(values)), OperandState::Valid(right)) => {
            Truth::from(values.iter().any(|value| value == right))
        }
        (
            OperandState::Null | OperandState::Valid(_),
            OperandState::Null | OperandState::Valid(_),
        ) => Truth::Invalid,
        (OperandState::Absent | OperandState::Malformed(_), _)
        | (_, OperandState::Absent | OperandState::Malformed(_)) => unreachable_state(),
    }
}

fn text_test(
    left: OperandState<'_>,
    right: OperandState<'_>,
    test: fn(&str, &str) -> bool,
) -> Truth {
    match (left, right) {
        (OperandState::Valid(Value::Text(left)), OperandState::Valid(Value::Text(right))) => {
            Truth::from(test(left, right))
        }
        (
            OperandState::Null | OperandState::Valid(_),
            OperandState::Null | OperandState::Valid(_),
        ) => Truth::Invalid,
        (OperandState::Absent | OperandState::Malformed(_), _)
        | (_, OperandState::Absent | OperandState::Malformed(_)) => unreachable_state(),
    }
}

fn is_one_of(left: OperandState<'_>, right: OperandState<'_>) -> Truth {
    match (left, right) {
        (OperandState::Valid(left), OperandState::Valid(Value::List(options)))
            if !options.is_empty() =>
        {
            Truth::from(options.iter().any(|option| option == left))
        }
        (OperandState::Null, OperandState::Valid(Value::List(options))) if !options.is_empty() => {
            Truth::from(options.iter().any(|option| option == &Value::Null))
        }
        (
            OperandState::Null | OperandState::Valid(_),
            OperandState::Null | OperandState::Valid(_),
        ) => Truth::Invalid,
        (OperandState::Absent | OperandState::Malformed(_), _)
        | (_, OperandState::Absent | OperandState::Malformed(_)) => unreachable_state(),
    }
}

#[derive(Clone, Copy)]
enum OrderingTest {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

fn ordering(left: OperandState<'_>, right: OperandState<'_>, test: OrderingTest) -> Truth {
    let (OperandState::Valid(left), OperandState::Valid(right)) = (left, right) else {
        return Truth::Invalid;
    };
    let comparison = match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => Some(left.cmp(right)),
        (Value::Decimal(left), Value::Decimal(right)) => Some(left.cmp(right)),
        (Value::Text(left), Value::Text(right)) => Some(left.cmp(right)),
        (Value::Date(left), Value::Date(right)) => Some(left.cmp(right)),
        (Value::DateTime(left), Value::DateTime(right)) => Some(left.cmp(right)),
        (Value::Duration(left), Value::Duration(right)) => Some(left.cmp(right)),
        _ => None,
    };
    comparison.map_or(Truth::Invalid, |comparison| {
        Truth::from(match test {
            OrderingTest::Less => comparison == Ordering::Less,
            OrderingTest::LessOrEqual => comparison != Ordering::Greater,
            OrderingTest::Greater => comparison == Ordering::Greater,
            OrderingTest::GreaterOrEqual => comparison != Ordering::Less,
        })
    })
}

const fn unreachable_state() -> Truth {
    Truth::Invalid
}

impl From<bool> for Truth {
    fn from(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use rulery_contracts::{DecimalValue, DurationValue, PolicyDate, StableId, TypeId, ValueKind};

    use super::*;

    #[test]
    fn presence_predicates_distinguish_all_fact_states() {
        let path = FactPath::from_str("member.status").expect("fact path");
        let error = FactValidationError::TypeMismatch {
            expected: TypeId::new("type.status").expect("type"),
            actual: ValueKind::Integer,
        };
        let value = Value::Text("active".to_owned());
        let states = [
            OperandState::Absent,
            OperandState::Null,
            OperandState::Malformed(&error),
            OperandState::Valid(&value),
        ];
        let predicates = [
            PresencePredicate::IsAbsent,
            PresencePredicate::IsPresent,
            PresencePredicate::IsValid,
            PresencePredicate::IsInvalid,
        ];
        let expected = [
            [Truth::True, Truth::False, Truth::False, Truth::False],
            [Truth::False, Truth::True, Truth::True, Truth::True],
            [Truth::Unknown, Truth::False, Truth::False, Truth::True],
            [Truth::Unknown, Truth::False, Truth::True, Truth::False],
        ];

        for (predicate_index, predicate) in predicates.iter().copied().enumerate() {
            for (state_index, state) in states.iter().copied().enumerate() {
                assert_eq!(
                    evaluate_presence(predicate, state),
                    expected[predicate_index][state_index]
                );
            }
        }

        let facts = BTreeMap::from([
            (path.clone(), FactEvidence::Malformed(error.clone())),
            (
                FactPath::from_str("member.name").expect("name path"),
                FactEvidence::Null,
            ),
        ]);
        let malformed = lookup_operand(&facts, &path);
        assert_eq!(malformed.path, path);
        assert!(matches!(malformed.state, OperandState::Malformed(found) if found == &error));
        assert_eq!(
            evaluate_presence(PresencePredicate::IsPresent, malformed.state),
            Truth::True
        );

        let missing_path = FactPath::from_str("member.missing").expect("missing path");
        let missing = lookup_operand(&facts, &missing_path);
        assert_eq!(missing.path, missing_path);
        assert_eq!(missing.state, OperandState::Absent);
        assert_eq!(
            evaluate_presence(PresencePredicate::IsValid, missing.state),
            Truth::Unknown
        );

        let null = lookup_operand(
            &facts,
            &FactPath::from_str("member.name").expect("name path"),
        );
        assert_eq!(
            evaluate_presence(PresencePredicate::IsAbsent, null.state),
            Truth::False
        );
    }

    #[test]
    fn operators_are_structural_type_strict_and_unicode_based() {
        let null = OperandState::Null;
        let explicit_null = Value::Null;
        let one = Value::Integer(1);
        let one_decimal = Value::Decimal(DecimalValue::parse("1.0").expect("decimal"));
        let two = Value::Integer(2);
        let composed = Value::Text("éclair".to_owned());
        let decomposed = Value::Text("e\u{301}clair".to_owned());

        assert_eq!(
            evaluate_binary(BinaryPredicate::Equals, null, null),
            Truth::True
        );
        assert_eq!(
            evaluate_binary(BinaryPredicate::NotEquals, null, null),
            Truth::False
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::Equals,
                null,
                OperandState::Valid(&explicit_null)
            ),
            Truth::True
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::Equals,
                OperandState::Valid(&one),
                OperandState::Valid(&one_decimal)
            ),
            Truth::False
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::LessThan,
                OperandState::Valid(&one),
                OperandState::Valid(&two)
            ),
            Truth::True
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::LessThan,
                OperandState::Valid(&one),
                OperandState::Valid(&one_decimal)
            ),
            Truth::Invalid
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::Equals,
                OperandState::Valid(&composed),
                OperandState::Valid(&decomposed)
            ),
            Truth::False
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::GreaterThan,
                OperandState::Valid(&composed),
                OperandState::Valid(&decomposed)
            ),
            Truth::True
        );

        let text = Value::Text("naïve café".to_owned());
        let contiguous = Value::Text("ïve".to_owned());
        let prefix = Value::Text("naï".to_owned());
        let suffix = Value::Text("café".to_owned());
        assert_eq!(
            binary(BinaryPredicate::Contains, &text, &contiguous),
            Truth::True
        );
        assert_eq!(
            binary(BinaryPredicate::StartsWith, &text, &prefix),
            Truth::True
        );
        assert_eq!(
            binary(BinaryPredicate::EndsWith, &text, &suffix),
            Truth::True
        );

        let list = Value::List(vec![Value::Integer(1), Value::Text("1".to_owned())]);
        assert_eq!(binary(BinaryPredicate::Contains, &list, &one), Truth::True);
        assert_eq!(
            binary(BinaryPredicate::Contains, &list, &one_decimal),
            Truth::False
        );
        assert_eq!(
            binary(BinaryPredicate::NotContains, &list, &one_decimal),
            Truth::True
        );

        let record_a = Value::Record(BTreeMap::from([
            (StableId::new("a").expect("a"), Value::Integer(1)),
            (StableId::new("b").expect("b"), Value::Integer(2)),
        ]));
        let record_b = Value::Record(BTreeMap::from([
            (StableId::new("b").expect("b"), Value::Integer(2)),
            (StableId::new("a").expect("a"), Value::Integer(1)),
        ]));
        assert_eq!(
            binary(BinaryPredicate::Equals, &record_a, &record_b),
            Truth::True
        );
        let ordered_list = Value::List(vec![Value::Integer(1), Value::Integer(2)]);
        let reversed_list = Value::List(vec![Value::Integer(2), Value::Integer(1)]);
        assert_eq!(
            binary(BinaryPredicate::Equals, &ordered_list, &reversed_list),
            Truth::False
        );

        let options = Value::List(vec![Value::Integer(1), Value::Integer(2)]);
        let empty = Value::List(Vec::new());
        assert_eq!(
            binary(BinaryPredicate::IsOneOf, &two, &options),
            Truth::True
        );
        assert_eq!(
            binary(BinaryPredicate::IsOneOf, &two, &empty),
            Truth::Invalid
        );

        let date_a = Value::Date(PolicyDate::parse("2026-01-01").expect("date"));
        let date_b = Value::Date(PolicyDate::parse("2026-01-02").expect("date"));
        let duration_a = Value::Duration(DurationValue::parse("1").expect("duration"));
        let duration_b = Value::Duration(DurationValue::parse("2").expect("duration"));
        assert_eq!(
            binary(BinaryPredicate::LessThan, &date_a, &date_b),
            Truth::True
        );
        assert_eq!(
            binary(BinaryPredicate::LessOrEqual, &duration_a, &duration_b),
            Truth::True
        );

        let missing = OperandState::Absent;
        let malformed_error = FactValidationError::OutOfRange;
        let malformed = OperandState::Malformed(&malformed_error);
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::NotContains,
                missing,
                OperandState::Valid(&one)
            ),
            Truth::Unknown
        );
        assert_eq!(
            evaluate_binary(
                BinaryPredicate::NotEquals,
                malformed,
                OperandState::Valid(&one)
            ),
            Truth::Invalid
        );
    }

    fn binary(predicate: BinaryPredicate, left: &Value, right: &Value) -> Truth {
        evaluate_binary(
            predicate,
            OperandState::Valid(left),
            OperandState::Valid(right),
        )
    }
}
