//! Operator, literal, and action type checking.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{FactPath, StableId, Value};
use rulery_ir::Operator;

/// Minimal checked type model for operator validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckedType {
    /// Boolean type.
    Boolean,
    /// Integer type.
    Integer,
    /// Decimal type.
    Decimal,
    /// Text type.
    Text,
    /// Date type.
    Date,
    /// Date-time type.
    DateTime,
    /// Duration type.
    Duration,
    /// Enum type id.
    Enum(StableId),
    /// List type.
    List(Box<CheckedType>),
}

/// One predicate operand with an attached checked type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperandSpec {
    /// Fact operand.
    Fact {
        /// Fact path identifier.
        path: FactPath,
        /// Checked fact type.
        ty: CheckedType,
    },
    /// Literal operand.
    Literal {
        /// Literal value.
        value: Value,
        /// Checked literal type.
        ty: CheckedType,
    },
    /// Reserved token `today`.
    ReservedToday,
    /// Reserved token `now`.
    ReservedNow,
}

/// Declared action parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionParamSpec {
    /// Declared parameter type.
    pub ty: CheckedType,
    /// Whether parameter is required.
    pub required: bool,
}

/// Action call to validate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionCallSpec {
    /// Passed arguments keyed by parameter name.
    pub args: BTreeMap<StableId, OperandSpec>,
}

/// Type-checking failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypecheckError {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

/// Validates one predicate operator application.
///
/// # Errors
///
/// Returns [`TypecheckError`] with code `RUL005` when operand arity or types are incompatible.
#[allow(clippy::too_many_lines)]
pub fn typecheck_predicate(
    operator: Operator,
    left: &OperandSpec,
    right: Option<&OperandSpec>,
) -> Result<(), TypecheckError> {
    let left_ty = operand_type(left);
    let right_ty = right.map(operand_type);

    let same_type_ok = right_ty
        .as_ref()
        .is_some_and(|right_ty| right_ty == &left_ty);

    match operator {
        Operator::Exists | Operator::Missing | Operator::IsTrue | Operator::IsFalse => {
            if right.is_some() {
                return Err(type_mismatch("unary operator received right operand"));
            }
            if matches!(operator, Operator::IsTrue | Operator::IsFalse)
                && left_ty != CheckedType::Boolean
            {
                return Err(type_mismatch("is_true/is_false require boolean"));
            }
        }
        Operator::Equals | Operator::NotEquals => {
            if !same_type_ok {
                return Err(type_mismatch("equality requires same-typed operands"));
            }
        }
        Operator::LessThan
        | Operator::LessOrEqual
        | Operator::GreaterThan
        | Operator::GreaterOrEqual => {
            if !same_type_ok {
                return Err(type_mismatch("ordering requires same-typed operands"));
            }
            if !matches!(left_ty, CheckedType::Integer | CheckedType::Decimal) {
                return Err(type_mismatch("ordering supports integer/decimal only"));
            }
        }
        Operator::Contains => {
            let Some(right_ty) = right_ty else {
                return Err(type_mismatch("contains requires right operand"));
            };
            match left_ty {
                CheckedType::Text => {
                    if right_ty != CheckedType::Text {
                        return Err(type_mismatch("text contains requires text right operand"));
                    }
                }
                CheckedType::List(inner) => {
                    if *inner != right_ty {
                        return Err(type_mismatch(
                            "list contains requires element-typed operand",
                        ));
                    }
                }
                _ => return Err(type_mismatch("contains supports text or list operands")),
            }
        }
        Operator::StartsWith | Operator::EndsWith | Operator::Matches => {
            if right_ty != Some(CheckedType::Text) || left_ty != CheckedType::Text {
                return Err(type_mismatch(
                    "starts_with/ends_with/matches require text operands",
                ));
            }
        }
        Operator::IsOneOf => {
            let Some(right_ty) = right_ty else {
                return Err(type_mismatch("is_one_of requires right operand"));
            };
            if !same_type_ok {
                return Err(type_mismatch("is_one_of requires same-typed operands"));
            }
            if matches!(right, Some(OperandSpec::Literal { value: Value::List(values), .. }) if values.is_empty())
            {
                return Err(type_mismatch("is_one_of list literal must be non-empty"));
            }
            if right_ty == CheckedType::List(Box::new(left_ty.clone())) {
                return Err(type_mismatch("is_one_of expects scalar right operand"));
            }
        }
        Operator::Before | Operator::After | Operator::OnOrBefore | Operator::OnOrAfter => {
            let Some(right_ty) = right_ty else {
                return Err(type_mismatch("temporal operator requires right operand"));
            };
            if !matches!(left_ty, CheckedType::Date | CheckedType::DateTime) {
                return Err(type_mismatch(
                    "temporal left operand must be date/date-time",
                ));
            }
            if !matches!(right_ty, CheckedType::Date | CheckedType::DateTime) {
                return Err(type_mismatch(
                    "temporal right operand must be date/date-time",
                ));
            }
        }
        Operator::Between => {
            if !matches!(left_ty, CheckedType::Date | CheckedType::DateTime) {
                return Err(type_mismatch(
                    "between requires date/date-time left operand",
                ));
            }
            if right_ty != Some(left_ty.clone()) {
                return Err(type_mismatch("between requires same-typed bounds"));
            }
        }
    }

    Ok(())
}

/// Validates action-call arguments against declared parameters.
///
/// # Errors
///
/// Returns [`TypecheckError`] with code `RUL005` on missing, unknown, or type-mismatched args.
pub fn typecheck_action(
    params: &BTreeMap<StableId, ActionParamSpec>,
    call: &ActionCallSpec,
) -> Result<(), TypecheckError> {
    for (name, parameter) in params {
        if parameter.required && !call.args.contains_key(name) {
            return Err(type_mismatch(format!("missing required argument `{name}`")));
        }
    }

    let param_names = params.keys().collect::<BTreeSet<_>>();
    for (name, value) in &call.args {
        let Some(parameter) = params.get(name) else {
            return Err(type_mismatch(format!("unknown action argument `{name}`")));
        };
        let value_ty = operand_type(value);
        if value_ty != parameter.ty {
            return Err(type_mismatch(format!(
                "action argument `{name}` has wrong type"
            )));
        }
    }
    let call_names = call.args.keys().collect::<BTreeSet<_>>();
    if !call_names.is_subset(&param_names) {
        return Err(type_mismatch("action call includes undeclared arguments"));
    }

    Ok(())
}

fn operand_type(operand: &OperandSpec) -> CheckedType {
    match operand {
        OperandSpec::Fact { ty, .. } | OperandSpec::Literal { ty, .. } => ty.clone(),
        OperandSpec::ReservedToday => CheckedType::Date,
        OperandSpec::ReservedNow => CheckedType::DateTime,
    }
}

fn type_mismatch(message: impl Into<String>) -> TypecheckError {
    TypecheckError {
        code: rulery_diagnostics::DiagnosticCode::TYPE_MISMATCH,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rulery_contracts::FactPath;

    use super::*;

    #[test]
    fn typechecker_enforces_operator_matrix() {
        let boolean_fact = fact("member.active", CheckedType::Boolean);
        let int_fact = fact("member.age", CheckedType::Integer);
        let text_fact = fact("member.tier", CheckedType::Text);
        let list_fact = fact(
            "member.tags",
            CheckedType::List(Box::new(CheckedType::Text)),
        );
        let date_fact = fact("member.training.valid-until", CheckedType::Date);
        let datetime_fact = fact("member.last-login", CheckedType::DateTime);
        let enum_fact = fact(
            "member.state",
            CheckedType::Enum(StableId::new("type.member_state").expect("type id")),
        );
        let enum_fact_other = fact(
            "member.previous-state",
            CheckedType::Enum(StableId::new("type.member_state").expect("type id")),
        );

        assert!(typecheck_predicate(Operator::Equals, &int_fact, Some(&int_lit(18))).is_ok());
        assert!(
            typecheck_predicate(Operator::NotEquals, &enum_fact, Some(&enum_fact_other)).is_ok()
        );
        assert!(typecheck_predicate(Operator::LessThan, &int_fact, Some(&int_lit(21))).is_ok());
        assert!(typecheck_predicate(Operator::Contains, &text_fact, Some(&text_lit("go"))).is_ok());
        assert!(
            typecheck_predicate(Operator::Contains, &list_fact, Some(&text_lit("vip"))).is_ok()
        );
        assert!(
            typecheck_predicate(Operator::StartsWith, &text_fact, Some(&text_lit("go"))).is_ok()
        );
        assert!(typecheck_predicate(Operator::EndsWith, &text_fact, Some(&text_lit("ld"))).is_ok());
        assert!(
            typecheck_predicate(Operator::IsOneOf, &text_fact, Some(&text_lit("gold"))).is_ok()
        );
        assert!(
            typecheck_predicate(
                Operator::Before,
                &date_fact,
                Some(&OperandSpec::ReservedToday)
            )
            .is_ok()
        );
        assert!(
            typecheck_predicate(
                Operator::After,
                &datetime_fact,
                Some(&OperandSpec::ReservedNow)
            )
            .is_ok()
        );
        assert!(typecheck_predicate(Operator::IsTrue, &boolean_fact, None).is_ok());

        let mismatch = typecheck_predicate(Operator::Equals, &int_fact, Some(&text_lit("x")));
        assert!(
            matches!(mismatch, Err(TypecheckError { code, .. }) if code == rulery_diagnostics::DiagnosticCode::TYPE_MISMATCH)
        );

        let non_empty_list = OperandSpec::Literal {
            value: Value::List(vec![Value::Text("gold".to_owned())]),
            ty: CheckedType::List(Box::new(CheckedType::Text)),
        };
        let empty_list = OperandSpec::Literal {
            value: Value::List(Vec::new()),
            ty: CheckedType::List(Box::new(CheckedType::Text)),
        };
        assert!(typecheck_predicate(Operator::IsOneOf, &text_fact, Some(&non_empty_list)).is_err());
        assert!(typecheck_predicate(Operator::IsOneOf, &text_fact, Some(&empty_list)).is_err());

        let params = BTreeMap::from([
            (
                StableId::new("channel").expect("channel"),
                ActionParamSpec {
                    ty: CheckedType::Text,
                    required: true,
                },
            ),
            (
                StableId::new("retries").expect("retries"),
                ActionParamSpec {
                    ty: CheckedType::Integer,
                    required: false,
                },
            ),
        ]);
        let ok_call = ActionCallSpec {
            args: BTreeMap::from([(
                StableId::new("channel").expect("channel"),
                text_lit("email"),
            )]),
        };
        assert!(typecheck_action(&params, &ok_call).is_ok());

        let missing_required = ActionCallSpec {
            args: BTreeMap::new(),
        };
        assert!(typecheck_action(&params, &missing_required).is_err());

        let unknown_arg = ActionCallSpec {
            args: BTreeMap::from([(StableId::new("other").expect("other"), text_lit("x"))]),
        };
        assert!(typecheck_action(&params, &unknown_arg).is_err());

        let wrong_type = ActionCallSpec {
            args: BTreeMap::from([(StableId::new("channel").expect("channel"), int_lit(1))]),
        };
        assert!(typecheck_action(&params, &wrong_type).is_err());
    }

    fn fact(path: &str, ty: CheckedType) -> OperandSpec {
        OperandSpec::Fact {
            path: FactPath::from_str(path).expect("fact path"),
            ty,
        }
    }

    fn text_lit(value: &str) -> OperandSpec {
        OperandSpec::Literal {
            value: Value::Text(value.to_owned()),
            ty: CheckedType::Text,
        }
    }

    fn int_lit(value: i64) -> OperandSpec {
        OperandSpec::Literal {
            value: Value::Integer(value),
            ty: CheckedType::Integer,
        }
    }
}
