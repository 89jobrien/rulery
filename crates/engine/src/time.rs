//! Clock and policy-time adapters.

use std::time::{SystemTime, UNIX_EPOCH};

use rulery_contracts::{PolicyDate, PolicyTimeZone, UtcInstant, Value};
use thiserror::Error;

use crate::{OperandState, Truth};

/// Clock port used by deterministic evaluation.
pub trait Clock: Send + Sync {
    /// Returns the current UTC instant.
    ///
    /// # Errors
    ///
    /// Returns [`TimeZoneError`] when the system value is outside the supported instant range.
    fn now(&self) -> Result<UtcInstant, TimeZoneError>;
}

/// Host system clock adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<UtcInstant, TimeZoneError> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| TimeZoneError::OutOfRange)?;
        let nanoseconds =
            i128::try_from(duration.as_nanos()).map_err(|_| TimeZoneError::OutOfRange)?;
        UtcInstant::new(nanoseconds).map_err(|_| TimeZoneError::OutOfRange)
    }
}

/// Deterministic fixed clock adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedClock {
    now: UtcInstant,
}

impl FixedClock {
    /// Creates a fixed clock.
    #[must_use]
    pub const fn new(now: UtcInstant) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Result<UtcInstant, TimeZoneError> {
        Ok(self.now)
    }
}

/// Time-zone conversion port.
pub trait TimeZoneDatabase: Send + Sync {
    /// Returns a stable database implementation identity.
    fn identity(&self) -> &str;

    /// Converts a UTC instant to its local policy date.
    ///
    /// # Errors
    ///
    /// Returns [`TimeZoneError`] for unavailable zones/data or out-of-range values.
    fn local_date(
        &self,
        instant: UtcInstant,
        zone: &PolicyTimeZone,
    ) -> Result<PolicyDate, TimeZoneError>;
}

/// Jiff-backed IANA timezone database adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JiffTimeZoneDatabase {
    identity: String,
}

impl Default for JiffTimeZoneDatabase {
    fn default() -> Self {
        Self {
            identity: "jiff/system-tzdb".to_owned(),
        }
    }
}

impl JiffTimeZoneDatabase {
    /// Creates an adapter with an explicitly recorded database identity.
    #[must_use]
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
        }
    }
}

impl TimeZoneDatabase for JiffTimeZoneDatabase {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn local_date(
        &self,
        instant: UtcInstant,
        zone: &PolicyTimeZone,
    ) -> Result<PolicyDate, TimeZoneError> {
        let timestamp = jiff::Timestamp::new(
            i64::try_from(instant.as_nanoseconds().div_euclid(1_000_000_000))
                .map_err(|_| TimeZoneError::OutOfRange)?,
            i32::try_from(instant.as_nanoseconds().rem_euclid(1_000_000_000))
                .map_err(|_| TimeZoneError::OutOfRange)?,
        )
        .map_err(|_| TimeZoneError::OutOfRange)?;
        let timezone = jiff::tz::TimeZone::get(zone.as_str())
            .map_err(|_| TimeZoneError::UnknownZone(zone.as_str().to_owned()))?;
        PolicyDate::parse(timestamp.to_zoned(timezone).date().to_string())
            .map_err(|_| TimeZoneError::OutOfRange)
    }
}

/// Evaluated clock operand retained for traces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClockValue {
    /// Exact typed clock value.
    pub value: Value,
    /// Clock source identity.
    pub clock_identity: String,
    /// Timezone database identity when local-date conversion was used.
    pub timezone_database_identity: Option<String>,
}

impl ClockValue {
    /// Creates a UTC `now` operand.
    #[must_use]
    pub fn now(value: UtcInstant, clock_identity: impl Into<String>) -> Self {
        Self {
            value: Value::DateTime(value),
            clock_identity: clock_identity.into(),
            timezone_database_identity: None,
        }
    }

    /// Creates a local `today` operand.
    #[must_use]
    pub fn today(
        value: PolicyDate,
        clock_identity: impl Into<String>,
        timezone_database_identity: impl Into<String>,
    ) -> Self {
        Self {
            value: Value::Date(value),
            clock_identity: clock_identity.into(),
            timezone_database_identity: Some(timezone_database_identity.into()),
        }
    }
}

/// Date expiry boundary policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateExpiryPolicy {
    /// A date expires only before today.
    Inclusive,
    /// A date expires on or before today.
    Exclusive,
}

/// Evaluates temporal strict ordering for same-kind date or UTC-instant operands.
#[must_use]
pub fn evaluate_before(left: OperandState<'_>, right: OperandState<'_>) -> Truth {
    temporal_compare(left, right, |ordering| ordering == std::cmp::Ordering::Less)
}

/// Evaluates temporal greater-than-or-equal ordering for same-kind operands.
#[must_use]
pub fn evaluate_on_or_after(left: OperandState<'_>, right: OperandState<'_>) -> Truth {
    temporal_compare(left, right, |ordering| ordering != std::cmp::Ordering::Less)
}

/// Evaluates date expiry against a policy-local `today` value.
#[must_use]
pub fn evaluate_expired(
    fact: OperandState<'_>,
    today: &PolicyDate,
    policy: DateExpiryPolicy,
) -> Truth {
    match fact {
        OperandState::Absent => Truth::Unknown,
        OperandState::Valid(Value::Date(date)) => Truth::from(match policy {
            DateExpiryPolicy::Inclusive => date < today,
            DateExpiryPolicy::Exclusive => date <= today,
        }),
        OperandState::Malformed(_) | OperandState::Null | OperandState::Valid(_) => Truth::Invalid,
    }
}

/// Evaluates decisive date unexpiry while preserving unknown and invalid states.
#[must_use]
pub fn evaluate_unexpired(
    fact: OperandState<'_>,
    today: &PolicyDate,
    policy: DateExpiryPolicy,
) -> Truth {
    evaluate_expired(fact, today, policy).not()
}

fn temporal_compare(
    left: OperandState<'_>,
    right: OperandState<'_>,
    predicate: impl FnOnce(std::cmp::Ordering) -> bool,
) -> Truth {
    match (left, right) {
        (OperandState::Absent, _) | (_, OperandState::Absent) => Truth::Unknown,
        (OperandState::Valid(Value::Date(left)), OperandState::Valid(Value::Date(right))) => {
            Truth::from(predicate(left.cmp(right)))
        }
        (
            OperandState::Valid(Value::DateTime(left)),
            OperandState::Valid(Value::DateTime(right)),
        ) => Truth::from(predicate(left.cmp(right))),
        (OperandState::Malformed(_) | OperandState::Null, _)
        | (_, OperandState::Malformed(_) | OperandState::Null)
        | (OperandState::Valid(_), OperandState::Valid(_)) => Truth::Invalid,
    }
}

/// Timezone adapter failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TimeZoneError {
    /// Requested IANA zone is unavailable.
    #[error("unknown timezone `{0}`")]
    UnknownZone(String),
    /// Required timezone data is unavailable.
    #[error("timezone data is unavailable")]
    DataUnavailable,
    /// Instant or local date is outside the supported range.
    #[error("time value is outside the supported range")]
    OutOfRange,
}

#[cfg(test)]
mod tests {
    use rulery_contracts::{PolicyTimeZone, TimeValueError};

    use super::*;

    #[test]
    fn temporal_predicates_use_policy_local_date() {
        let database = JiffTimeZoneDatabase::new("jiff/test-tzdb-2026a");
        let zone = PolicyTimeZone::new("America/New_York").expect("zone");
        let before_transition =
            UtcInstant::new(1_772_944_200_i128 * 1_000_000_000).expect("instant");
        let after_transition =
            UtcInstant::new(1_772_955_000_i128 * 1_000_000_000).expect("instant");
        assert_eq!(
            database
                .local_date(before_transition, &zone)
                .expect("local date")
                .to_string(),
            "2026-03-07"
        );
        let today = database
            .local_date(after_transition, &zone)
            .expect("local date");
        assert_eq!(today.to_string(), "2026-03-08");

        let today_operand =
            ClockValue::today(today.clone(), "fixed/test", database.identity().to_owned());
        assert_eq!(
            today_operand.timezone_database_identity.as_deref(),
            Some("jiff/test-tzdb-2026a")
        );
        let now_operand = ClockValue::now(after_transition, "fixed/test");
        assert_eq!(now_operand.value, Value::DateTime(after_transition));
        assert!(now_operand.timezone_database_identity.is_none());

        let prior = Value::Date(PolicyDate::parse("2026-03-07").expect("date"));
        let boundary = Value::Date(today.clone());
        assert_eq!(
            evaluate_expired(
                OperandState::Valid(&prior),
                &today,
                DateExpiryPolicy::Inclusive
            ),
            Truth::True
        );
        assert_eq!(
            evaluate_expired(
                OperandState::Valid(&boundary),
                &today,
                DateExpiryPolicy::Inclusive
            ),
            Truth::False
        );
        assert_eq!(
            evaluate_expired(
                OperandState::Valid(&boundary),
                &today,
                DateExpiryPolicy::Exclusive
            ),
            Truth::True
        );
        assert_eq!(
            evaluate_unexpired(
                OperandState::Valid(&boundary),
                &today,
                DateExpiryPolicy::Exclusive
            ),
            Truth::False
        );
        assert_eq!(
            evaluate_unexpired(OperandState::Absent, &today, DateExpiryPolicy::Inclusive),
            Truth::Unknown
        );

        let date = Value::Date(today);
        let instant = Value::DateTime(after_transition);
        assert_eq!(
            evaluate_before(OperandState::Valid(&date), OperandState::Valid(&instant)),
            Truth::Invalid
        );
        assert_eq!(
            evaluate_on_or_after(OperandState::Valid(&instant), OperandState::Valid(&instant)),
            Truth::True
        );

        let unknown = PolicyTimeZone::new("Mars/Olympus_Mons").expect("syntactic zone");
        assert!(matches!(
            database.local_date(after_transition, &unknown),
            Err(TimeZoneError::UnknownZone(_))
        ));

        let fixed = FixedClock::new(after_transition);
        assert_eq!(fixed.now().expect("fixed now"), after_transition);

        let invalid_date: Result<PolicyDate, TimeValueError> = PolicyDate::parse("0000-01-01");
        assert!(invalid_date.is_err());
        assert_eq!(
            TimeZoneError::DataUnavailable,
            TimeZoneError::DataUnavailable
        );
        assert_eq!(TimeZoneError::OutOfRange, TimeZoneError::OutOfRange);
    }
}
