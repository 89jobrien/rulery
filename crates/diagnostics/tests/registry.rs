//! Diagnostic registry contract tests.

use rulery_diagnostics::{
    DiagnosticCode, DiagnosticFamily, EvidenceRequirement, Severity, diagnostic_definition,
    diagnostic_definitions,
};

#[test]
fn registry_contains_exact_v01_definitions() {
    let definitions = diagnostic_definitions();
    assert_eq!(definitions.len(), 40);
    let unique: std::collections::BTreeSet<_> = definitions
        .iter()
        .map(|entry| entry.code().as_str())
        .collect();
    assert_eq!(unique.len(), 40);

    let syntax = DiagnosticCode::new(DiagnosticCode::SYNTAX_INVALID).expect("code");
    let syntax_definition = diagnostic_definition(&syntax).expect("known definition");
    assert_eq!(syntax.family(), DiagnosticFamily::Syntax);
    assert_eq!(syntax_definition.severity(), Severity::Error);
    assert_eq!(syntax_definition.evidence(), &EvidenceRequirement::None);

    let internal = DiagnosticCode::new(DiagnosticCode::INTERNAL_INVARIANT).expect("code");
    assert_eq!(internal.family(), DiagnosticFamily::Internal);
    assert!(diagnostic_definition(&internal).is_some());
    assert_eq!(
        DiagnosticCode::new("RUL899").expect("reserved").family(),
        DiagnosticFamily::Reserved
    );
    assert_eq!(
        DiagnosticCode::new("RUL901").expect("internal").family(),
        DiagnosticFamily::Internal
    );
    assert!(DiagnosticCode::new("RUL01").is_err());
    assert!(DiagnosticCode::new("rul001").is_err());
}
