//! Procedural macros for typed Rulery integrations.
//!
//! [`RuleFacts`] derives a consuming `TryFrom` conversion into `CaseFacts` while requiring explicit
//! roots and field paths. Generated code resolves the facade crate hygienically, including when a
//! downstream package renames its `rulery` dependency.

#![forbid(unsafe_code)]

extern crate proc_macro;

mod rule_facts;

use proc_macro::TokenStream;

/// Derives consuming conversion from a Rust record to Rulery case facts.
///
/// The struct requires `#[rulery(root = "...")]`; every named field requires an explicit
/// `#[rulery(path = "...")]`. Supported field types are `bool`, `i64`, `String`, `Value`, and
/// `Option<T>` around those types. `Option::None` omits the field instead of producing explicit
/// null. Duplicate, malformed, nested, unknown, and unsupported declarations are compile errors.
///
/// Generated code resolves the downstream `rulery` facade through `proc-macro-crate`, so renamed
/// dependencies remain hygienic. Generic and tuple structs are not supported.
#[proc_macro_derive(RuleFacts, attributes(rulery))]
pub fn derive_rule_facts(input: TokenStream) -> TokenStream {
    rule_facts::derive(input.into()).into()
}

#[cfg(test)]
mod tests {
    use quote::quote;

    #[test]
    fn rule_facts_requires_explicit_valid_paths() {
        let valid = quote! {
            #[rulery(root = "member")]
            struct MemberFacts {
                #[rulery(path = "active")]
                active: bool,
                #[rulery(path = "name")]
                name: Option<String>,
            }
        };
        assert!(super::rule_facts::expand(&syn::parse2(valid).expect("input")).is_ok());

        for invalid in [
            quote! { struct MissingRoot { #[rulery(path = "x")] x: bool } },
            quote! { #[rulery(root = "member")] struct MissingPath { x: bool } },
            quote! { #[rulery(root = "member", unknown = "x")] struct Unknown { #[rulery(path = "x")] x: bool } },
            quote! { #[rulery(root = "member")] struct BadType { #[rulery(path = "x")] x: std::collections::HashMap<String, String> } },
        ] {
            assert!(super::rule_facts::expand(&syn::parse2(invalid).expect("input")).is_err());
        }
    }
}
