use proc_macro::TokenStream as TS1;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use either::{Either, Left, Right};
use inflector::Inflector;
use proc_macro2::{ Span, TokenStream as TS2 };
use quote::{quote, ToTokens};
use syn::{Attribute, Expr, Ident, ItemStruct, ItemType, LitStr, parse_macro_input, Token, Type, TypeParamBound};
use serde::Deserialize;
use syn::punctuated::Punctuated;

#[derive(Deserialize, Clone, Debug)]
struct Config {
    #[serde(flatten)]
    crates: HashMap<String, Defn>,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct Defn {
    #[serde(flatten)]
    vals: HashMap<String, toml::Value>,
}

#[derive(Clone)]
struct PeripheralField {
    ident: Ident,
//    original_type: Type,
//    altered_type: Type,
    alias: ItemType,
    impls: Option<Punctuated<TypeParamBound, Token![+]>>,
    field_value: Either<Ident, toml::Value>,
    attrs: Vec<Attribute>
}

impl PeripheralField {
    fn new(struct_ident: Ident, ident: Ident, original_type: Type, section_definition: &Defn, attrs: Vec<Attribute>) -> Self {
        let alias_value = struct_ident.to_string() + ident.to_string().to_class_case().as_str();

        let alias_value = Ident::new(
            alias_value.as_str(),
            Span::call_site(),
        );

        let config = section_definition.vals.get(&ident.to_string()).unwrap_or_else(|| panic!("Board config for field {:?} missing", ident.to_string()));

        let mut impls: Option<Punctuated<TypeParamBound, Token![+]>> = None;

        let mut should_be_const = false;

        let altered_type = match original_type {
            Type::ImplTrait(ref ty) => {
                let toml::Value::String(t) = config else {
                    panic!("Type of {:?} in board-config.toml is not a string", ident.to_string());
                };

                impls = Some(ty.bounds.clone());

                syn::parse_str::<Type>(t.as_str()).expect("Exp:6")
            },
            Type::Tuple(_) => {
                let toml::Value::String(t) = config else {
                    panic!("Type of {:?} in board-config.toml is not a string", ident.to_string());
                };

                syn::parse_str::<Type>(t.as_str()).expect("Exp:7")
            },
            _ => {
                should_be_const = true;
                original_type.clone()
            }
        };
        let alias_type = altered_type.clone();
        let alias: ItemType =
            syn::parse2(quote! { type #alias_value = #alias_type; }).expect("Exp:8");

        let field_value = if should_be_const {
            Right(config.clone())
        } else {
            match altered_type.clone() {
                Type::Path(ty) => {
                    let ident = &ty.path.segments.last().unwrap().ident;
                    Left(ident.clone())
                }
                _ => panic!("For some reason {:?} shouldn't be a const, but also isn't a path", ident.to_string()),
            }
        };

        PeripheralField {
            ident,
//            original_type,
//            altered_type,
            alias,
            impls,
            field_value,
            attrs
        }
    }
}

/// Mark a struct as a resource for extraction from the `Peripherals` instance.
#[proc_macro_attribute]
pub fn board_cfg(args: TS1, item: TS1) -> TS1 {
    let mut s: ItemStruct = syn::parse2(item.into()).expect("Resource item must be a struct.");

    let root_path = find_root_path();
    let cfg_path = root_path.clone();
    let cfg_path = cfg_path.as_ref().and_then(|c| {
        let mut x = c.to_owned();
        x.push("board-cfg.toml");
        Some(x)
    });

    let input = parse_macro_input!(args as LitStr);
    let section = input.value();

    let maybe_cfg = cfg_path.as_ref().and_then(|c| load_crate_cfg(&c));

    let Some(cfg) = maybe_cfg else {
        panic!("Couldn't find board-config.toml");
    };

    let Some(defs) = cfg.crates.get(&section) else {
        panic!("board-config.toml doesn't contain a section for {}", section);
    };

    let cfg_path_binding = cfg_path.unwrap();
    let cfg_path_str = cfg_path_binding.to_str().unwrap();

    let macro_ident = Ident::new(
        inflector::cases::snakecase::to_snake_case(s.ident.to_string().as_str()).as_str(),
        Span::call_site(),
    );

    let field_data: HashMap<Ident, PeripheralField> = s.fields.iter().cloned().map(|f| {
        PeripheralField::new(s.ident.clone(), f.ident.expect("Exp:10"), f.ty, defs, f.attrs)
    }).map(|p| (p.ident.clone(), p.clone())).collect();

    let aliases: Vec<ItemType> = field_data.iter().map(|(_, f)| f.alias.clone()).collect();

    let ident = &s.ident;

    s.fields.iter_mut().for_each(
        |field| {
            let ident = &field.ident.clone().expect("Exp:2");

            let alias_ident = field_data.get(ident).expect("Exp:3").alias.clone().ident;

            field.ty = syn::parse2(quote! { #alias_ident }).expect("Exp:4");
        });

    let field_idents: Vec<Ident> = field_data.iter().map(|(i, _)| i.clone()).collect();

    let field_types: Vec<TS2> = field_data.iter().map(|(_, fd)|
        match fd.field_value.clone() {
            Left(v) => quote! { $P.#v },
            Right(v) => {
                let t_string = v.to_string();
                syn::parse_str::<Expr>(&t_string).expect(&format!(
                    "Failed to parse `{}` as a valid token!",
                    &t_string
                )).to_token_stream()
            }
        }
    ).collect();

    let mut where_clauses = field_data.iter().map(|(_, fd)| {
        if fd.impls.is_none() {
            return None;
        }
        let bounds_tokens = fd.impls.iter().map(|bound| bound.to_token_stream());

        let alias_ident = &fd.alias.ident;

        Some(quote! { #alias_ident: #(#bounds_tokens)+* })
    }).filter_map(|p| p).peekable();

    let impl_clause = if where_clauses.peek().is_none() {
        quote! { }
    } else {
        quote! { impl #ident where #(#where_clauses),* {} }
    };

    let field_attrs =
        field_data.iter().map(|(_, fd)| &fd.attrs);
    let doc = format!(
        "Extract `{}` from a `Peripherals` instance.",
        ident.to_string()
    );

    let toml_recompile_hack_mod = ident.to_string().to_snake_case() + "_toml_recompile_hack";
    let toml_recompile_hack_mod = Ident::new(
        toml_recompile_hack_mod.as_str(),
        Span::call_site(),
    );

    let q =
    quote! {
        #(
            #aliases
        )*

        #s

        #impl_clause

        #[doc = #doc]
        macro_rules! #macro_ident {
            ( $P:ident ) => {
                #ident {
                    #(
                        #(
                            #field_attrs
                        )*
                        #field_idents: #field_types
                    ),*
                }
            };
        }

        mod #toml_recompile_hack_mod {
            const _: &[u8] = include_bytes!(#cfg_path_str);
        }
    };

    q.into()

}

fn load_crate_cfg(path: &Path) -> Option<Config> {
    let contents = std::fs::read_to_string(&path).ok()?;

    let parsed = toml::from_str::<Config>(&contents).ok()?;

    Some(parsed)
}

// From https://stackoverflow.com/q/60264534
fn find_root_path() -> Option<PathBuf> {
    // First we get the arguments for the rustc invocation
    let mut args = std::env::args();

    // Then we loop through them all, and find the value of "out-dir"
    let mut out_dir = None;
    while let Some(arg) = args.next() {
        if arg == "--out-dir" {
            out_dir = args.next();
        }
    }

    // Finally we clean out_dir by removing all trailing directories, until it ends with target
    let mut out_dir = PathBuf::from(out_dir?);
    while !out_dir.ends_with("target") {
        if !out_dir.pop() {
            // We ran out of directories...
            return None;
        }
    }

    out_dir.pop();

    Some(out_dir)
}