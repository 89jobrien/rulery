//! Exact scalar contract tests.

use rulery_contracts::{
    DecimalValue, DurationValue, LanguageVersion, PolicyDate, PolicyTimeZone,
    TimeZoneDatabaseIdentity, UtcInstant,
};

#[test]
fn exact_scalars_use_canonical_wire_forms() {
    for (input, expected) in [
        ("-0", "0"),
        ("0.0", "0"),
        ("12.3400", "12.34"),
        ("-12.3400", "-12.34"),
    ] {
        assert_eq!(
            DecimalValue::parse(input).expect("decimal").as_str(),
            expected
        );
    }
    for invalid in ["+1", "01", "1e2", "1E2", ".1", "1."] {
        assert!(DecimalValue::parse(invalid).is_err(), "accepted {invalid}");
    }

    assert!(PolicyDate::parse("2024-02-29").is_ok());
    for invalid in ["2023-02-29", "0000-01-01", "10000-01-01", "2024-2-01"] {
        assert!(PolicyDate::parse(invalid).is_err(), "accepted {invalid}");
    }

    let epoch = UtcInstant::new(0).expect("epoch");
    assert_eq!(epoch.to_string(), "0");
    assert_eq!(epoch.to_rfc3339(), "1970-01-01T00:00:00.000000000Z");
    assert!(UtcInstant::parse("+1").is_err());
    assert!(UtcInstant::parse("01").is_err());
    assert_eq!(
        DurationValue::parse("-42").expect("duration").to_string(),
        "-42"
    );

    assert!(PolicyTimeZone::new("America/New_York").is_ok());
    assert!(PolicyTimeZone::new("").is_err());
    assert!(TimeZoneDatabaseIdentity::new("jiff", "2026a").is_ok());
    assert!(TimeZoneDatabaseIdentity::new("", "2026a").is_err());
    assert_eq!(LanguageVersion::V1.get(), 1);
}
