//! Derive macro for [`envconfig`](https://docs.rs/envconfig).
//!
//! Go's `envconfig` inspects a specification struct at run time with `reflect`
//! and reads `envconfig:"…"` struct tags. Rust has no run-time reflection, so
//! the equivalent information is gathered at compile time from
//! `#[envconfig(…)]` attributes and turned into a static description of the
//! specification.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

mod names;
mod types;

/// Options collected from one field's `#[envconfig(…)]` attribute.
#[derive(Default)]
struct FieldOpts {
    /// `#[envconfig(name = "…")]` — the alternate environment variable name.
    name: Option<String>,
    /// `#[envconfig(field_name = "…")]` — overrides the declared field name.
    field_name: Option<String>,
    default: Option<String>,
    required: Option<String>,
    ignored: Option<String>,
    split_words: Option<String>,
    desc: Option<String>,
    nested: bool,
    embedded: bool,
}

/// Go's `strconv.ParseBool` truthiness, used for the boolean-valued tags.
fn is_true(s: &Option<String>) -> bool {
    matches!(
        s.as_deref(),
        Some("1" | "t" | "T" | "TRUE" | "true" | "True")
    )
}

fn parse_opts(attrs: &[syn::Attribute]) -> syn::Result<FieldOpts> {
    let mut opts = FieldOpts::default();
    for attr in attrs {
        if !attr.path().is_ident("envconfig") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            // Every key accepts either the bare flag form (`ignored`) or the
            // string form (`ignored = "true"`), mirroring Go's tag values.
            let value = || -> syn::Result<String> {
                if meta.input.peek(syn::Token![=]) {
                    let v: syn::LitStr = meta.value()?.parse()?;
                    Ok(v.value())
                } else {
                    Ok("true".to_owned())
                }
            };
            let ident = meta
                .path
                .get_ident()
                .ok_or_else(|| meta.error("expected an identifier"))?
                .to_string();
            match ident.as_str() {
                "name" => opts.name = Some(value()?),
                "field_name" => opts.field_name = Some(value()?),
                "default" => opts.default = Some(value()?),
                "required" => opts.required = Some(value()?),
                "ignored" => opts.ignored = Some(value()?),
                "split_words" => opts.split_words = Some(value()?),
                "desc" => opts.desc = Some(value()?),
                "nested" => opts.nested = true,
                "embedded" => opts.embedded = true,
                other => {
                    return Err(meta.error(format!("unknown envconfig option `{other}`")));
                }
            }
            Ok(())
        })?;
    }
    Ok(opts)
}

/// Returns `Some(inner)` when `ty` is `Wrapper<inner>`.
fn generic_inner<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != wrapper {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// True for `Vec<u8>`, which Go decodes as raw bytes rather than a
/// comma-separated list.
fn is_byte_vec(ty: &Type) -> bool {
    generic_inner(ty, "Vec")
        .and_then(|inner| match inner {
            Type::Path(p) => p.path.segments.last(),
            _ => None,
        })
        .is_some_and(|seg| seg.ident == "u8")
}

/// Derives an `EnvConfig` implementation for a specification struct.
#[proc_macro_derive(EnvConfig, attributes(envconfig))]
pub fn derive_env_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "EnvConfig can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "EnvConfig can only be derived for structs with named fields",
        ));
    };

    let mut gather = Vec::new();
    let mut process = Vec::new();

    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let opts = parse_opts(&field.attrs)?;

        // Go skips a field entirely when `ignored` is true, along with
        // everything nested inside it.
        if is_true(&opts.ignored) {
            continue;
        }

        let ty = &field.ty;
        let declared = opts
            .field_name
            .clone()
            .unwrap_or_else(|| names::pascal_case(&ident.to_string()));

        // Go derives the key from the field name, optionally splitting it into
        // words; an explicit alternate name replaces the result.
        let base = if is_true(&opts.split_words) {
            names::split_words(&declared)
        } else {
            declared.clone()
        };
        let alt = opts.name.clone().unwrap_or_default().to_uppercase();
        let default = opts.default.clone().unwrap_or_default();
        let required = opts.required.clone().unwrap_or_default();
        let desc = opts.desc.clone().unwrap_or_default();

        if opts.embedded || opts.nested {
            // A struct field that does not decode itself contributes no
            // variable of its own; its fields are gathered instead. An
            // embedded field keeps the parent prefix, a named nested field
            // uses its own derived key as the prefix.
            if opts.embedded {
                gather.push(quote! {
                    ::envconfig::EnvConfig::gather(&self.#ident, prefix, out);
                });
                process.push(quote! {
                    ::envconfig::EnvConfig::process_env(&mut self.#ident, prefix)?;
                });
            } else {
                gather.push(quote! {
                    ::envconfig::EnvConfig::gather(
                        &self.#ident,
                        &::envconfig::make_key(prefix, #base, #alt),
                        out,
                    );
                });
                process.push(quote! {
                    ::envconfig::EnvConfig::process_env(
                        &mut self.#ident,
                        &::envconfig::make_key(prefix, #base, #alt),
                    )?;
                });
            }
            continue;
        }

        let type_desc = types::describe(ty);
        let type_name = types::render(ty);

        gather.push(quote! {
            out.push(::envconfig::VarInfo {
                name: #declared,
                alt: #alt,
                key: ::envconfig::make_key(prefix, #base, #alt),
                type_desc: #type_desc,
                default: #default,
                required: #required,
                desc: #desc,
            });
        });

        // Go dereferences a pointer field and decodes into the pointee,
        // allocating it if needed. `Option<T>` is the Rust counterpart.
        let slot = if generic_inner(ty, "Option").is_some() {
            quote! { self.#ident.get_or_insert_with(::core::default::Default::default) }
        } else {
            quote! { &mut self.#ident }
        };

        // Go's `[]byte` takes the raw value instead of being comma-split. It
        // cannot be a second `impl` on `Vec<T>`, so it is called directly.
        let decode_call = if is_byte_vec(ty) {
            quote! { ::envconfig::decode::decode_byte_slice(__slot, &__value) }
        } else {
            quote! { ::envconfig::dispatch::probe(__slot).dispatch(&__value) }
        };

        process.push(quote! {
            {
                let __key = ::envconfig::make_key(prefix, #base, #alt);
                if let ::core::option::Option::Some(__value) =
                    ::envconfig::resolve(&__key, #alt, #default, #required)?
                {
                    let __slot = #slot;
                    if let ::core::result::Result::Err(__err) = #decode_call {
                        return ::core::result::Result::Err(::envconfig::Error::Parse(
                            ::envconfig::ParseError {
                                key_name: __key,
                                field_name: #declared,
                                type_name: #type_name,
                                value: __value,
                                err: __err,
                            },
                        ));
                    }
                }
            }
        });
    }

    // A specification whose fields are all ignored uses none of its
    // parameters; keep the generated code warning-clean.
    let gather_unused = gather.is_empty().then(|| quote! { let _ = (prefix, out); });
    let process_unused = process.is_empty().then(|| quote! { let _ = prefix; });

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::envconfig::EnvConfig for #name #ty_generics #where_clause {
            fn gather(&self, prefix: &str, out: &mut ::std::vec::Vec<::envconfig::VarInfo>) {
                #gather_unused
                #(#gather)*
            }

            fn process_env(&mut self, prefix: &str)
                -> ::core::result::Result<(), ::envconfig::Error>
            {
                // Brings the ladder's dispatch method into scope without
                // adding a name to the caller's namespace.
                use ::envconfig::dispatch::Dispatch as _;
                #process_unused
                #(#process)*
                ::core::result::Result::Ok(())
            }
        }
    })
}
