#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, parse_macro_input, parse_quote};

#[proc_macro_derive(Fingerprint)]
pub fn derive_fingerprint(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let fields = match &input.data {
        Data::Struct(data) => &data.fields,
        _ => return syn::Error::new_spanned(name, "Fingerprint supports structs only").to_compile_error().into(),
    };
    let accesses: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let member = field.ident.clone().map(syn::Member::Named).unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index)));
            let ty = &field.ty;
            generics.make_where_clause().predicates.push(parse_quote!(#ty: ::craft_cache::Fingerprint));
            quote!(::craft_cache::Fingerprint::write_fingerprint(&self.#member, hasher);)
        })
        .collect();
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    quote! {
        impl #impl_generics ::craft_cache::Fingerprint for #name #type_generics #where_clause {
            fn write_fingerprint(&self, hasher: &mut ::craft_cache::FingerprintHasher) {
                #(#accesses)*
            }
        }
    }
    .into()
}
