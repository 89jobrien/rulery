//! Canonical hash conformance tests.

use std::fmt::Write as _;

use proptest::prelude::*;

use rulery_contracts::{ContentHash, HashDomain, hash_parts};

/// Every domain must hash differently over the same parts, for every variant.
#[test]
fn every_domain_produces_a_distinct_identity() {
    let mut identities = HashDomain::ALL
        .into_iter()
        .map(|domain| hash_parts(domain, [b"{}".as_slice()]))
        .collect::<Vec<_>>();
    let count = identities.len();
    identities.sort_unstable();
    identities.dedup();
    assert_eq!(
        identities.len(),
        count,
        "two domains collided over identical parts, so separation is not holding"
    );
}

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
    assert!(ContentHash::parse(format!("blake3:{}", "A".repeat(64))).is_err());
    assert!(ContentHash::parse("blake3:00").is_err());
}

fn domain() -> impl Strategy<Value = HashDomain> {
    prop::sample::select(HashDomain::ALL.to_vec())
}

/// Short byte payloads, which is the size the framing contract actually has to disambiguate.
fn payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..24)
}

fn digest() -> impl Strategy<Value = [u8; 32]> {
    prop::array::uniform32(any::<u8>())
}

proptest! {
    /// The same domain and parts always produce the same hash.
    ///
    /// This is the whole basis for reproducible traces: a byte-identical input has to produce a
    /// byte-identical identity, every time, on every machine.
    #[test]
    fn hashing_is_deterministic(
        selected in domain(),
        parts in prop::collection::vec(payload(), 0..4),
    ) {
        let slices = parts.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let first = hash_parts(selected, slices.iter().copied());
        let second = hash_parts(selected, slices.iter().copied());
        prop_assert_eq!(first, second);
    }

    /// Two domains never agree over the same parts.
    #[test]
    fn domains_are_separated(
        parts in prop::collection::vec(payload(), 0..4),
        first in 0usize..HashDomain::ALL.len(),
        second in 0usize..HashDomain::ALL.len(),
    ) {
        let slices = parts.iter().map(Vec::as_slice).collect::<Vec<_>>();
        if HashDomain::ALL[first] != HashDomain::ALL[second] {
            prop_assert_ne!(
                hash_parts(HashDomain::ALL[first], slices.iter().copied()),
                hash_parts(HashDomain::ALL[second], slices.iter().copied()),
            );
        }
    }

    /// The length prefix makes the partition recoverable, so re-splitting the same bytes is a
    /// different input.
    ///
    /// Without the prefix, `["ab", "c"]` and `["a", "bc"]` would frame identically and two different
    /// packages would collide on one identity. This is the property the framing exists to provide.
    #[test]
    fn framing_distinguishes_a_split_from_a_join(
        selected in domain(),
        left in payload(),
        right in payload(),
    ) {
        let joined = [left.as_slice(), right.as_slice()].concat();
        prop_assert_ne!(
            hash_parts(selected, [left.as_slice(), right.as_slice()]),
            hash_parts(selected, [joined.as_slice()]),
        );
    }

    /// Part order is part of the input.
    #[test]
    fn part_order_changes_the_hash(
        selected in domain(),
        left in payload(),
        right in payload(),
    ) {
        if left != right {
            prop_assert_ne!(
                hash_parts(selected, [left.as_slice(), right.as_slice()]),
                hash_parts(selected, [right.as_slice(), left.as_slice()]),
            );
        }
    }

    /// An empty part list is distinct from a single empty part, for the same framing reason.
    #[test]
    fn an_empty_part_list_is_not_one_empty_part(selected in domain()) {
        prop_assert_ne!(
            hash_parts(selected, [] as [&[u8]; 0]),
            hash_parts(selected, [b"".as_slice()]),
        );
    }

    /// Every digest round-trips through its exact wire form.
    #[test]
    fn every_digest_round_trips_through_its_wire_form(bytes in digest()) {
        let hash = ContentHash::from_bytes(bytes);
        prop_assert_eq!(ContentHash::parse(hash.to_string()).expect("round trip"), hash);
        prop_assert_eq!(hash.to_hex().len(), 64);
    }

    /// The wire form is lowercase, and uppercase is rejected for any digest whatsoever.
    #[test]
    fn the_wire_form_rejects_uppercase_for_every_digest(bytes in digest()) {
        let hash = ContentHash::from_bytes(bytes);
        let mut uppercase = String::with_capacity(64);
        for byte in hash.as_bytes() {
            write!(uppercase, "{byte:02X}").expect("string write");
        }
        prop_assert!(
            ContentHash::parse(format!("blake3:{uppercase}")).is_err(),
            "uppercase hexadecimal must be rejected for every digest"
        );
        prop_assert_eq!(hash.to_hex(), hash.to_hex().to_lowercase());
    }
}
