//! `RuleFacts` derive implementation.

use std::collections::BTreeSet;

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericArgument, LitStr, PathArguments, Type};

pub(crate) fn derive(input: TokenStream) -> TokenStream {
    match syn::parse2(input).and_then(|input| expand(&input)) {
        Ok(tokens) => tokens,
        Err(error) => error.into_compile_error(),
    }
}

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let root = parse_value(&input.attrs, "root")?.ok_or_else(|| {
        syn::Error::new_spanned(&input.ident, "missing #[rulery(root = \"...\")]")
    })?;
    validate_path(&root)?;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "RuleFacts requires a struct",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "RuleFacts requires named fields",
        ));
    };
    let facade = facade_path();
    let mut seen = BTreeSet::new();
    let mut conversions = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let path = parse_value(&field.attrs, "path")?
            .ok_or_else(|| syn::Error::new_spanned(field, "missing #[rulery(path = \"...\")]"))?;
        validate_path(&path)?;
        if !seen.insert(path.clone()) {
            return Err(syn::Error::new_spanned(
                field,
                "duplicate rulery field path",
            ));
        }
        let relative = path.strip_prefix(&format!("{root}.")).unwrap_or(&path);
        if relative.contains('.') {
            return Err(syn::Error::new_spanned(
                field,
                "nested field paths are not supported",
            ));
        }
        let key = LitStr::new(relative, proc_macro2::Span::call_site());
        let (inner, optional) = option_inner(&field.ty);
        if optional {
            let conversion = value_conversion(inner, quote!(value), &facade)?;
            conversions.push(quote! {
                if let ::core::option::Option::Some(value) = source.#ident {
                    let key = #facade::__private::StableId::new(#key).map_err(|error| #facade::FactBuildError { field: #key.to_owned(), message: error.to_string() })?;
                    fields.insert(key, #conversion);
                }
            });
        } else {
            let conversion = value_conversion(inner, quote!(source.#ident), &facade)?;
            conversions.push(quote! {
                let key = #facade::__private::StableId::new(#key).map_err(|error| #facade::FactBuildError { field: #key.to_owned(), message: error.to_string() })?;
                fields.insert(key, #conversion);
            });
        }
    }
    let name = &input.ident;
    let root = LitStr::new(&root, proc_macro2::Span::call_site());
    Ok(quote! {
        impl ::core::convert::TryFrom<#name> for #facade::__private::CaseFacts {
            type Error = #facade::FactBuildError;
            fn try_from(source: #name) -> ::core::result::Result<Self, Self::Error> {
                let mut fields = #facade::__private::BTreeMap::new();
                #(#conversions)*
                let root = #facade::__private::FactRootId::new(#root).map_err(|error| #facade::FactBuildError { field: "root".to_owned(), message: error.to_string() })?;
                let mut roots = #facade::__private::BTreeMap::new();
                roots.insert(root, #facade::__private::Value::Record(fields));
                Ok(#facade::__private::CaseFacts::new(roots))
            }
        }
    })
}

fn parse_value(attrs: &[syn::Attribute], expected: &str) -> syn::Result<Option<String>> {
    let mut value = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("rulery")) {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident(expected) {
                return Err(meta.error("unknown rulery attribute"));
            }
            if value.is_some() {
                return Err(meta.error("duplicate rulery attribute"));
            }
            value = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        })?;
    }
    Ok(value)
}

fn validate_path(path: &str) -> syn::Result<()> {
    let valid = !path.is_empty()
        && path.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().enumerate().all(|(index, byte)| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit() && index > 0
                        || matches!(byte, b'-' | b'_') && index > 0
                })
        });
    valid
        .then_some(())
        .ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "malformed rulery path"))
}

fn option_inner(ty: &Type) -> (&Type, bool) {
    let Type::Path(path) = ty else {
        return (ty, false);
    };
    let Some(segment) = path.path.segments.last() else {
        return (ty, false);
    };
    if segment.ident != "Option" {
        return (ty, false);
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return (ty, false);
    };
    let Some(GenericArgument::Type(inner)) = arguments.args.first() else {
        return (ty, false);
    };
    (inner, true)
}

fn value_conversion(
    ty: &Type,
    expression: TokenStream,
    facade: &TokenStream,
) -> syn::Result<TokenStream> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "unsupported RuleFacts field type",
        ));
    };
    match path
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
        .as_deref()
    {
        Some("bool") => Ok(quote!(#facade::__private::Value::Boolean(#expression))),
        Some("i64") => Ok(quote!(#facade::__private::Value::Integer(#expression))),
        Some("String") => Ok(quote!(#facade::__private::Value::Text(#expression))),
        Some("Value") => Ok(expression),
        _ => Err(syn::Error::new_spanned(
            ty,
            "unsupported RuleFacts field type",
        )),
    }
}

fn facade_path() -> TokenStream {
    match crate_name("rulery") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{name}");
            quote!(::#ident)
        }
        Err(_) => quote!(::rulery),
    }
}
