//! Declarative validated literal macros.

/// Error produced while constructing facts from declarative syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactBuildError {
    /// Rejected field or constructor.
    pub field: String,
    /// Validation message.
    pub message: String,
}

impl std::fmt::Display for FactBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for FactBuildError {}

/// Error produced while constructing a source scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioBuildError {
    /// Invalid field.
    pub field: String,
    /// Validation message.
    pub message: String,
}

impl std::fmt::Display for ScenarioBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ScenarioBuildError {}

#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn build_scenario(
    id: &str,
    title: String,
    decision: &str,
    at: rulery_contracts::UtcInstant,
    given: std::collections::BTreeMap<rulery_contracts::FactPath, rulery_contracts::Value>,
    expect: rulery_scenarios::SourceExpectedDecision,
    span: rulery_contracts::Span,
    tags: Vec<&str>,
) -> Result<rulery_scenarios::ScenarioSource, ScenarioBuildError> {
    if title.trim().is_empty() {
        return Err(ScenarioBuildError {
            field: "title".to_owned(),
            message: "scenario title must not be empty".to_owned(),
        });
    }
    let convert = |field: &str, message: String| ScenarioBuildError {
        field: field.to_owned(),
        message,
    };
    let id =
        rulery_contracts::ScenarioId::new(id).map_err(|error| convert("id", error.to_string()))?;
    let decision = rulery_contracts::DecisionId::new(decision)
        .map_err(|error| convert("decision", error.to_string()))?;
    let tags = tags
        .into_iter()
        .map(|tag| {
            rulery_contracts::StableId::new(tag).map_err(|error| convert("tags", error.to_string()))
        })
        .collect::<Result<_, _>>()?;
    Ok(rulery_scenarios::ScenarioSource {
        id,
        title,
        description: None,
        decision,
        at,
        given,
        expect,
        tags,
        span,
        root_authored: true,
    })
}

#[doc(hidden)]
pub fn assert_decision_contract(
    actual: &rulery_scenarios::ActualDecision,
    outcome: rulery_contracts::OutcomeKind,
    determining: &std::collections::BTreeSet<rulery_contracts::QualifiedRuleId>,
    required_facts: &std::collections::BTreeSet<rulery_contracts::FactPath>,
    reasons: &std::collections::BTreeSet<rulery_contracts::ReasonCode>,
) {
    let mut mismatches = Vec::new();
    if actual.outcome != outcome {
        mismatches.push(format!(
            "outcome: expected {outcome:?}, actual {:?}",
            actual.outcome
        ));
    }
    if &actual.determining_rules != determining {
        mismatches.push(format!(
            "determining: expected {determining:?}, actual {:?}",
            actual.determining_rules
        ));
    }
    if &actual.required_facts != required_facts {
        mismatches.push(format!(
            "required_facts: expected {required_facts:?}, actual {:?}",
            actual.required_facts
        ));
    }
    if &actual.reason_codes != reasons {
        mismatches.push(format!(
            "reasons: expected {reasons:?}, actual {:?}",
            actual.reason_codes
        ));
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[macro_export]
/// Constructs a validated, unexecuted source scenario.
macro_rules! scenario {
    (id: $id:literal, title: $title:expr, decision: $decision:literal, at: $at:expr, given: $given:expr, expect: $expect:expr, span: $span:expr $(, tags: [$($tag:literal),* $(,)?])? $(,)?) => {{
        $crate::__private::build_scenario($id, ($title).to_string(), $decision, $at, $given, $expect, $span, vec![$($($tag),*)?])
    }};
}

#[macro_export]
/// Asserts all four exact decision expectation fields.
macro_rules! assert_decision {
    ($actual:expr, outcome: $outcome:ident, determining: [$($rule:literal),* $(,)?], required_facts: [$($path:literal),* $(,)?], reasons: [$($reason:literal),* $(,)?] $(,)?) => {{
        $crate::__private::assert_decision_contract(
            &$actual,
            $crate::__private::OutcomeKind::$outcome,
            &[ $(<$crate::__private::QualifiedRuleId as ::std::str::FromStr>::from_str($rule).expect("invalid determining rule")),* ].into_iter().collect(),
            &[ $(<$crate::__private::FactPath as ::std::str::FromStr>::from_str($path).expect("invalid required fact")),* ].into_iter().collect(),
            &[ $($crate::__private::ReasonCode::new($reason).expect("invalid reason code")),* ].into_iter().collect(),
        )
    }};
}

#[macro_export]
/// Builds a registry-derived diagnostic with caller evidence.
macro_rules! diagnostic {
    { code: $code:expr, message: $message:expr, primary: $primary:expr, evidence: [$($evidence:expr),* $(,)?] $(,)? } => {{
        (|| {
            let mut builder = $crate::__private::Diagnostic::builder($code, $message)?
                .push_label($primary);
            $(builder = builder.push_evidence($evidence);)*
            builder.build()
        })()
    }};
    { code: $code:expr, message: $message:expr, primary: $primary:expr $(,)? } => {{
        (|| {
            $crate::__private::Diagnostic::builder($code, $message)?
                .push_label($primary)
                .build()
        })()
    }};
}

#[macro_export]
/// Constructs a validated [`StableId`](rulery_contracts::StableId) literal.
macro_rules! stable_id {
    ($value:literal) => {{ $crate::__private::StableId::new($value).expect("invalid stable_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`PackageId`](rulery_contracts::PackageId) literal.
macro_rules! package_id {
    ($value:literal) => {{ $crate::__private::PackageId::new($value).expect("invalid package_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`DecisionId`](rulery_contracts::DecisionId) literal.
macro_rules! decision_id {
    ($value:literal) => {{ $crate::__private::DecisionId::new($value).expect("invalid decision_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`RuleId`](rulery_contracts::RuleId) literal.
macro_rules! rule_id {
    ($value:literal) => {{ $crate::__private::RuleId::new($value).expect("invalid rule_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`ActionId`](rulery_contracts::ActionId) literal.
macro_rules! action_id {
    ($value:literal) => {{ $crate::__private::ActionId::new($value).expect("invalid action_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`ScenarioId`](rulery_contracts::ScenarioId) literal.
macro_rules! scenario_id {
    ($value:literal) => {{ $crate::__private::ScenarioId::new($value).expect("invalid scenario_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`EscalationId`](rulery_contracts::EscalationId) literal.
macro_rules! escalation_id {
    ($value:literal) => {{ $crate::__private::EscalationId::new($value).expect("invalid escalation_id! literal") }};
}
#[macro_export]
/// Constructs a validated [`ReasonCode`](rulery_contracts::ReasonCode) literal.
macro_rules! reason_code {
    ($value:literal) => {{ $crate::__private::ReasonCode::new($value).expect("invalid reason_code! literal") }};
}
#[macro_export]
/// Constructs a validated [`FactPath`](rulery_contracts::FactPath) literal.
macro_rules! fact_path {
    ($value:literal) => {{
        <$crate::__private::FactPath as ::std::str::FromStr>::from_str($value)
            .expect("invalid fact_path! literal")
    }};
}
#[macro_export]
/// Constructs typed root facts without vocabulary inference.
macro_rules! facts {
    ({ $($tokens:tt)* }) => {{
        (|| -> ::std::result::Result<$crate::__private::CaseFacts, $crate::FactBuildError> {
            let mut roots = $crate::__private::BTreeMap::new();
            $crate::__facts_entries!(roots; $($tokens)*);
            Ok($crate::__private::CaseFacts::new(roots))
        })()
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __facts_entries {
    ($map:ident;) => {};
    ($map:ident; $key:ident : $value:tt $(, $($rest:tt)*)?) => {{
        let root = $crate::__private::FactRootId::new(stringify!($key)).map_err(|error| $crate::FactBuildError { field: stringify!($key).to_owned(), message: error.to_string() })?;
        let value = $crate::__fact_value!($value)?;
        $map.insert(root, value);
        $crate::__facts_entries!($map; $($($rest)*)?);
    }};
    ($map:ident; $key:ident : $ctor:ident ( $($args:literal),* ) $(, $($rest:tt)*)?) => {{
        let root = $crate::__private::FactRootId::new(stringify!($key)).map_err(|error| $crate::FactBuildError { field: stringify!($key).to_owned(), message: error.to_string() })?;
        let value = $crate::__fact_value!($ctor($($args),*))?;
        $map.insert(root, value);
        $crate::__facts_entries!($map; $($($rest)*)?);
    }};
    ($map:ident; $key:ident : [ $($inner:tt)* ] $(, $($rest:tt)*)?) => {{
        let root = $crate::__private::FactRootId::new(stringify!($key)).map_err(|error| $crate::FactBuildError { field: stringify!($key).to_owned(), message: error.to_string() })?;
        let value = $crate::__fact_value!([$($inner)*])?;
        $map.insert(root, value);
        $crate::__facts_entries!($map; $($($rest)*)?);
    }};
    ($map:ident; $key:ident : { $($inner:tt)* } $(, $($rest:tt)*)?) => {{
        let root = $crate::__private::FactRootId::new(stringify!($key)).map_err(|error| $crate::FactBuildError { field: stringify!($key).to_owned(), message: error.to_string() })?;
        let value = $crate::__fact_value!({$($inner)*})?;
        $map.insert(root, value);
        $crate::__facts_entries!($map; $($($rest)*)?);
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __fact_value {
    (null) => { Ok::<_, $crate::FactBuildError>($crate::__private::Value::Null) };
    (true) => { Ok::<_, $crate::FactBuildError>($crate::__private::Value::Boolean(true)) };
    (false) => { Ok::<_, $crate::FactBuildError>($crate::__private::Value::Boolean(false)) };
    (decimal($value:literal)) => { $crate::__private::DecimalValue::parse($value).map($crate::__private::Value::Decimal).map_err(|error| $crate::FactBuildError { field: "decimal".to_owned(), message: error.to_string() }) };
    (text($value:literal)) => { Ok::<_, $crate::FactBuildError>($crate::__private::Value::Text($value.to_owned())) };
    (date($value:literal)) => { $crate::__private::PolicyDate::parse($value).map($crate::__private::Value::Date).map_err(|error| $crate::FactBuildError { field: "date".to_owned(), message: error.to_string() }) };
    (date_time($value:literal)) => { $crate::__private::UtcInstant::parse($value).map($crate::__private::Value::DateTime).map_err(|error| $crate::FactBuildError { field: "date_time".to_owned(), message: error.to_string() }) };
    (duration($value:literal)) => { $crate::__private::DurationValue::parse($value).map($crate::__private::Value::Duration).map_err(|error| $crate::FactBuildError { field: "duration".to_owned(), message: error.to_string() }) };
    (enum_value($type_id:literal, $variant:literal)) => {{
        let type_id = $crate::__private::TypeId::new($type_id).map_err(|error| $crate::FactBuildError { field: "enum_value".to_owned(), message: error.to_string() })?;
        let variant = $crate::__private::StableId::new($variant).map_err(|error| $crate::FactBuildError { field: "enum_value".to_owned(), message: error.to_string() })?;
        Ok::<_, $crate::FactBuildError>($crate::__private::Value::Enum($crate::__private::EnumValue::new(type_id, variant)))
    }};
    ([$($tokens:tt)*]) => {{
        let mut values = Vec::new();
        $crate::__fact_list! (values; $($tokens)*);
        Ok::<_, $crate::FactBuildError>($crate::__private::Value::List(values))
    }};
    ({$($tokens:tt)*}) => {{
        let mut fields = $crate::__private::BTreeMap::new();
        $crate::__fact_record! (fields; $($tokens)*);
        Ok::<_, $crate::FactBuildError>($crate::__private::Value::Record(fields))
    }};
    ($value:literal) => { Ok::<_, $crate::FactBuildError>($crate::__private::Value::Integer($value)) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __fact_list {
    ($values:ident;) => {};
    ($values:ident; $ctor:ident($($args:literal),*) $(, $($rest:tt)*)?) => {{ $values.push($crate::__fact_value!($ctor($($args),*))?); $crate::__fact_list!($values; $($($rest)*)?); }};
    ($values:ident; $value:tt $(, $($rest:tt)*)?) => {{ $values.push($crate::__fact_value!($value)?); $crate::__fact_list!($values; $($($rest)*)?); }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __fact_record {
    ($fields:ident;) => {};
    ($fields:ident; $key:ident : $ctor:ident($($args:literal),*) $(, $($rest:tt)*)?) => {{
        let key = $crate::__private::StableId::new(stringify!($key)).map_err(|error| $crate::FactBuildError { field: stringify!($key).to_owned(), message: error.to_string() })?;
        $fields.insert(key, $crate::__fact_value!($ctor($($args),*))?);
        $crate::__fact_record!($fields; $($($rest)*)?);
    }};
    ($fields:ident; $key:ident : $value:tt $(, $($rest:tt)*)?) => {{
        let key = $crate::__private::StableId::new(stringify!($key)).map_err(|error| $crate::FactBuildError { field: stringify!($key).to_owned(), message: error.to_string() })?;
        $fields.insert(key, $crate::__fact_value!($value)?);
        $crate::__fact_record!($fields; $($($rest)*)?);
    }};
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use rulery_contracts::{
        ContentHash, DecimalValue, DecisionId, DurationValue, FactRootId, FactState,
        LanguageVersion, OutcomeKind, PackageId, PolicyDate, PolicyTimeZone, QualifiedRuleId,
        ReasonCode, RuleId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath, StableId,
        TimeZoneDatabaseIdentity, TypeId, UtcInstant, Value, Version,
    };
    use rulery_diagnostics::{
        DiagnosticCode, DiagnosticLabel, DiagnosticProducer, LabelStyle, Severity, Suppressibility,
    };
    use rulery_engine::DecisionTraceV1;
    use rulery_scenarios::{ActualDecision, SourceExpectedDecision};

    #[test]
    fn declarative_literals_use_validated_constructors() {
        assert_eq!(crate::stable_id!("member-status").as_str(), "member-status");
        assert_eq!(crate::package_id!("pkg.main").as_str(), "pkg.main");
        assert_eq!(
            crate::decision_id!("decision.main").as_str(),
            "decision.main"
        );
        assert_eq!(crate::rule_id!("rule.main").as_str(), "rule.main");
        assert_eq!(crate::action_id!("action.main").as_str(), "action.main");
        assert_eq!(
            crate::scenario_id!("scenario.main").as_str(),
            "scenario.main"
        );
        assert_eq!(crate::escalation_id!("review.main").as_str(), "review.main");
        assert_eq!(crate::reason_code!("allowed").as_str(), "allowed");
        assert_eq!(
            crate::fact_path!("member.status").to_string(),
            "member.status"
        );

        let built = crate::facts!({
            null_value: null,
            enabled: true,
            count: 7,
            amount: decimal("12.340"),
            name: text("Ada"),
            day: date("2026-09-18"),
            moment: date_time("1"),
            wait: duration("5"),
            state: enum_value("type.state", "active"),
            items: [text("a"), text("b"),],
            profile: { name: text("Ada"), active: false, },
        })
        .expect("facts");
        assert_eq!(
            built.root(&FactRootId::new("enabled").expect("root")),
            rulery_contracts::FactState::Valid(&Value::Boolean(true))
        );
        assert!(matches!(
            built.root(&FactRootId::new("profile").expect("root")),
            rulery_contracts::FactState::Valid(Value::Record(_))
        ));
        assert_eq!(
            built.root(&FactRootId::new("null_value").expect("root")),
            FactState::Null
        );
        assert_eq!(
            built.root(&FactRootId::new("count").expect("root")),
            FactState::Valid(&Value::Integer(7))
        );
        assert_eq!(
            built.root(&FactRootId::new("amount").expect("root")),
            FactState::Valid(&Value::Decimal(
                DecimalValue::parse("12.340").expect("decimal")
            ))
        );
        assert_eq!(
            built.root(&FactRootId::new("name").expect("root")),
            FactState::Valid(&Value::Text("Ada".to_owned()))
        );
        assert_eq!(
            built.root(&FactRootId::new("day").expect("root")),
            FactState::Valid(&Value::Date(PolicyDate::parse("2026-09-18").expect("date")))
        );
        assert_eq!(
            built.root(&FactRootId::new("moment").expect("root")),
            FactState::Valid(&Value::DateTime(UtcInstant::parse("1").expect("instant")))
        );
        assert_eq!(
            built.root(&FactRootId::new("wait").expect("root")),
            FactState::Valid(&Value::Duration(
                DurationValue::parse("5").expect("duration")
            ))
        );
        let expected_enum = Value::Enum(rulery_contracts::EnumValue::new(
            TypeId::new("type.state").expect("type"),
            StableId::new("active").expect("variant"),
        ));
        assert_eq!(
            built.root(&FactRootId::new("state").expect("root")),
            FactState::Valid(&expected_enum)
        );
        assert!(
            matches!(built.root(&FactRootId::new("items").expect("root")), FactState::Valid(Value::List(values)) if values.len() == 2)
        );
        assert!(crate::facts!({ bad: decimal("not-decimal"), }).is_err());
    }

    #[test]
    fn workflow_macros_enforce_exact_contracts() {
        let span = span();
        let expected = SourceExpectedDecision {
            outcome: OutcomeKind::Approve,
            determining_rules: BTreeSet::new(),
            required_facts: BTreeSet::new(),
            reason_codes: BTreeSet::new(),
        };
        let scenario = crate::scenario! {
            id: "scenario.main",
            title: "Main scenario",
            decision: "decision.main",
            at: UtcInstant::new(1).expect("instant"),
            given: BTreeMap::new(),
            expect: expected,
            span: span,
            tags: ["smoke",],
        }
        .expect("scenario");
        assert_eq!(scenario.title, "Main scenario");
        assert_eq!(scenario.span, span);
        assert!(scenario.root_authored);
        assert!(crate::scenario! {
            id: "scenario.empty",
            title: "",
            decision: "decision.main",
            at: UtcInstant::new(1).expect("instant"),
            given: BTreeMap::new(),
            expect: SourceExpectedDecision { outcome: OutcomeKind::Approve, determining_rules: BTreeSet::new(), required_facts: BTreeSet::new(), reason_codes: BTreeSet::new() },
            span: span,
        }.is_err());

        let panic = std::panic::catch_unwind(|| {
            crate::assert_decision!(
                actual_decision(),
                outcome: Deny,
                determining: ["pkg.main::rule.expected"],
                required_facts: ["member.email"],
                reasons: ["expected"],
            );
        })
        .expect_err("all fields mismatch");
        let message = panic.downcast_ref::<String>().expect("panic string");
        for field in ["outcome", "determining", "required_facts", "reasons"] {
            assert!(message.contains(field));
        }

        let primary = DiagnosticLabel::new(span, LabelStyle::Primary, None).expect("label");
        let diagnostic = crate::diagnostic! {
            code: DiagnosticCode::new("RUL001").expect("code"),
            message: "invalid YAML",
            primary: primary,
        }
        .expect("diagnostic");
        assert_eq!(diagnostic.title(), "Invalid source syntax");
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(
            diagnostic.properties().producer(),
            DiagnosticProducer::Parser
        );
        assert_eq!(
            diagnostic.properties().suppressibility(),
            Suppressibility::Never
        );
        let missing_evidence = crate::diagnostic! {
            code: DiagnosticCode::new("RUL200").expect("code"),
            message: "conflict",
            primary: DiagnosticLabel::new(span, LabelStyle::Primary, None).expect("label"),
        };
        assert!(missing_evidence.is_err());
    }

    fn actual_decision() -> ActualDecision {
        ActualDecision {
            outcome: OutcomeKind::Approve,
            determining_rules: BTreeSet::from([QualifiedRuleId::new(
                PackageId::new("pkg.main").expect("package"),
                RuleId::new("rule.actual").expect("rule"),
            )]),
            required_facts: BTreeSet::new(),
            reason_codes: BTreeSet::from([ReasonCode::new("actual").expect("reason")]),
            trace: DecisionTraceV1 {
                evaluation_id: ContentHash::from_bytes([1; 32]),
                package: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                decision: DecisionId::new("decision.main").expect("decision"),
                evaluated_at: UtcInstant::new(1).expect("instant"),
                timezone: PolicyTimeZone::new("UTC").expect("timezone"),
                timezone_database: TimeZoneDatabaseIdentity::new("test", "1").expect("database"),
                package_hash: ContentHash::from_bytes([2; 32]),
                facts_hash: ContentHash::from_bytes([3; 32]),
                compiler: "compiler".to_owned(),
                language_version: LanguageVersion::V1,
                outcome: None,
                determining_rules: Vec::new(),
                superseded_rules: Vec::new(),
                rule_traces: Vec::new(),
                missing_facts: BTreeSet::new(),
                invalid_facts: Vec::new(),
                strategy_applications: Vec::new(),
                conflict: None,
                trace_hash: ContentHash::from_bytes([4; 32]),
            },
        }
    }

    fn span() -> rulery_contracts::Span {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("scenario.yaml").expect("path"),
                Arc::<str>::from("x"),
            ),
        )
        .expect("source");
        map.span(SourceKey::new(1), 0, 1).expect("span")
    }
}
