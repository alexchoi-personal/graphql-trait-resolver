use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, DeriveInput, Expr, Lit, Meta};

#[proc_macro_derive(TraitResolver, attributes(resolver, batch_resolver))]
pub fn derive_trait_resolver(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    if let Some(resolver_attr) = find_attribute(&input.attrs, "resolver") {
        let resolver_name = extract_name_value(resolver_attr).unwrap_or_else(|| name.to_string());
        return generate_resolver_impl(name, &resolver_name);
    }

    if let Some(batch_attr) = find_attribute(&input.attrs, "batch_resolver") {
        let (resolver_name, batch_key) = extract_batch_resolver_attrs(batch_attr);
        return generate_batch_resolver_registration(name, &resolver_name, &batch_key);
    }

    syn::Error::new_spanned(name, "Missing #[resolver] or #[batch_resolver] attribute")
        .to_compile_error()
        .into()
}

fn find_attribute<'a>(attrs: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attrs.iter().find(|attr| attr.path().is_ident(name))
}

fn extract_name_value(attr: &Attribute) -> Option<String> {
    let meta = attr.meta.clone();
    if let Meta::List(list) = meta {
        let nested: Result<syn::punctuated::Punctuated<Meta, syn::Token![,]>, _> =
            list.parse_args_with(syn::punctuated::Punctuated::parse_terminated);
        if let Ok(nested) = nested {
            for meta in nested {
                if let Meta::NameValue(nv) = meta {
                    if nv.path.is_ident("name") {
                        if let Expr::Lit(expr_lit) = &nv.value {
                            if let Lit::Str(lit_str) = &expr_lit.lit {
                                return Some(lit_str.value());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_batch_resolver_attrs(attr: &Attribute) -> (String, String) {
    let mut resolver_name = String::new();
    let mut batch_key = String::new();

    if let Meta::List(list) = &attr.meta {
        let nested: Result<syn::punctuated::Punctuated<Meta, syn::Token![,]>, _> =
            list.parse_args_with(syn::punctuated::Punctuated::parse_terminated);
        if let Ok(nested) = nested {
            for meta in nested {
                if let Meta::NameValue(nv) = meta {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let Lit::Str(lit_str) = &expr_lit.lit {
                            if nv.path.is_ident("name") {
                                resolver_name = lit_str.value();
                            } else if nv.path.is_ident("batch_key") {
                                batch_key = lit_str.value();
                            }
                        }
                    }
                }
            }
        }
    }

    (resolver_name, batch_key)
}

fn generate_resolver_impl(name: &syn::Ident, resolver_name: &str) -> TokenStream {
    let expanded = quote! {
        impl ::graphql_resolver::Resolver for #name {
            fn name(&self) -> &'static str {
                #resolver_name
            }

            fn resolve<'a>(
                &'a self,
                ctx: &'a ::graphql_resolver::ResolverContext,
                args: ::std::collections::HashMap<String, ::async_graphql::Value>,
            ) -> ::graphql_resolver::BoxFuture<'a, ::graphql_resolver::ResolverResult<::async_graphql::Value>> {
                Box::pin(self.execute(ctx, args))
            }
        }

        ::graphql_resolver::inventory::submit! {
            ::graphql_resolver::ResolverRegistration::new(|| {
                Box::new(#name::default()) as Box<dyn ::graphql_resolver::Resolver>
            }, #resolver_name)
        }
    };
    TokenStream::from(expanded)
}

fn generate_batch_resolver_registration(
    name: &syn::Ident,
    resolver_name: &str,
    batch_key: &str,
) -> TokenStream {
    let expanded = quote! {
        ::graphql_resolver::inventory::submit! {
            ::graphql_resolver::BatchResolverRegistration::new(|| {
                Box::new(#name::default()) as Box<dyn ::graphql_resolver::ErasedBatchResolver>
            }, #resolver_name, #batch_key)
        }
    };
    TokenStream::from(expanded)
}
