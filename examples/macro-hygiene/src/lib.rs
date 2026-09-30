//! Proves the Rulery macros expand hygienically under a renamed dependency.
//!
//! The facade's declarative macros expand through `$crate::__private`, and `rulery-macros` locates
//! the facade with `proc_macro_crate`. Neither claim can be checked from inside this workspace,
//! because every in-tree test sees the dependency under its real name. This crate is the
//! downstream consumer: it depends on `rulery` as `rlry`, so if either expansion path hard-coded
//! `::rulery`, or if the procedural derive stopped resolving the facade by name, this would not
//! compile.
//!
//! There is deliberately no test function here. Compiling is the assertion, which is why
//! `xtask conformance` builds this crate with default features and with `macros` enabled rather
//! than running a suite against it. See `xtask/src/conformance.rs::check_macro_hygiene`.
//!
//! Both expansion paths are exercised. `stable_id!` and `facts!` are declarative, and `facts!`
//! reaches the deepest `$crate` surface of the two: the hidden module's re-exported contracts,
//! the crate-level error type, the shared `BTreeMap`, and the nested entry/value helper macros.
//! `RuleFacts` is procedural and is compiled only under the `macros` feature, matching the
//! facade, where the re-export is `#[cfg(feature = "macros")]` too.

/// Builds a validated stable identifier through a renamed `rulery` dependency.
///
/// Exercises `$crate::__private::StableId::new`, the shortest declarative expansion path.
pub fn renamed_stable_id() -> rlry::contracts::StableId {
    rlry::stable_id!("member-id")
}

/// Builds case facts through a renamed `rulery` dependency.
///
/// Exercises the whole declarative value pipeline: root construction, the record form, and the
/// `text` constructor, none of which resolve if `$crate` is not hygienic.
pub fn renamed_facts() -> rlry::contracts::CaseFacts {
    rlry::facts!({
        member: {
            active: true,
            name: text("Ada"),
        }
    })
    .expect("valid facts")
}

#[cfg(feature = "macros")]
pub mod procedural {
    use rlry::macros::RuleFacts;

    /// A record whose facts are derived against a renamed `rulery` dependency.
    ///
    /// The derive expands to `TryFrom<MemberFacts> for ::rlry::__private::CaseFacts`, so the field
    /// type, the attribute shape, and the facade's hidden module all have to resolve for this to
    /// build. A hard-coded `::rulery` path would fail here and nowhere else in the workspace.
    #[derive(RuleFacts)]
    #[rulery(root = "member")]
    pub struct MemberFacts {
        /// Whether the member is active.
        #[rulery(path = "active")]
        pub active: bool,
        /// The member's display name, omitted when absent.
        #[rulery(path = "name")]
        pub name: Option<String>,
    }

    /// Converts the derived record, proving the generated code is not just resolvable but usable.
    pub fn renamed_derived_facts() -> rlry::contracts::CaseFacts {
        rlry::contracts::CaseFacts::try_from(MemberFacts {
            active: true,
            name: Some("Ada".to_owned()),
        })
        .expect("conversion")
    }
}
