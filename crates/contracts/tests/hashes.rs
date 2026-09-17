//! Canonical hash conformance tests.

use rulery_contracts::{ContentHash, HashDomain, hash_parts};

#[test]
fn canonical_hash_vectors_match_specification() {
    let vectors = [
        (
            HashDomain::CompiledPackageV1,
            "1c6402278430173b5964f65d31c1ec06a9765c13c6e34bad762fcdee8ffbd6c8",
        ),
        (
            HashDomain::CaseFactsV1,
            "fa785b19b5601f57afd5548934d2c1e4b20bc1a8ab25b89e6bece83d041d31aa",
        ),
        (
            HashDomain::DecisionTraceV1,
            "4773d0e94079df21090915deecb6b2f41327122e6b479afb63d5d082f174aced",
        ),
    ];
    for (domain, expected) in vectors {
        assert_eq!(hash_parts(domain, [b"{}".as_slice()]).to_hex(), expected);
    }

    let package_hash = [0_u8; 32];
    let decision = b"checkout";
    let facts_hash = [0x11_u8; 32];
    let evaluated_at = 0_i128.to_be_bytes();
    let timezone = br#"{"implementation":"test","version":"1"}"#;
    let parts: [&[u8]; 5] = [
        &package_hash,
        decision,
        &facts_hash,
        &evaluated_at,
        timezone,
    ];
    assert_eq!(
        hash_parts(HashDomain::EvaluationV1, parts).to_hex(),
        "4c11eb70b89e8543605015c325e309fde68d9007c2086b97bad892693cae9279"
    );

    assert!(ContentHash::parse("sha256:00").is_err());
    assert!(ContentHash::parse(&format!("blake3:{}", "A".repeat(64))).is_err());
    assert!(ContentHash::parse("blake3:00").is_err());
}
