//! Procedural macros for Rulery.

#![forbid(unsafe_code)]

extern crate proc_macro;

mod rule_facts;

use proc_macro::TokenStream;

/// Derives conversion from a Rust record to Rulery case facts.
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
