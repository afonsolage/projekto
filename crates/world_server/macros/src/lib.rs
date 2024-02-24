use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Comma, Data, DataEnum, DeriveInput, Fields,
    Variant,
};

#[proc_macro_attribute]
pub fn message_source(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(item as DeriveInput);
    let name = &ast.ident;

    let Data::Enum(DataEnum { variants, .. }) = &ast.data else {
        panic!("message_source proc macro can only be used on enums");
    };

    let simplified_enum = generate_simplified_enum(name, variants);
    let structs = generate_structs(variants);
    let impls = generate_impls(name, variants);

    let expanded = quote! {
        #simplified_enum
        #structs
        #impls
    };

    expanded.into()
}

fn generate_simplified_enum(
    name: &Ident,
    variants: &Punctuated<Variant, Comma>,
) -> proc_macro2::TokenStream {
    let variant_names = variants.iter().map(|v| &v.ident);
    quote! {
        #[derive(Debug, Hash, Eq, PartialEq)]
        pub enum #name {
            #(#variant_names),*
        }
    }
}

fn generate_structs(variants: &Punctuated<Variant, Comma>) -> proc_macro2::TokenStream {
    let structs = variants.iter().map(|v| {
        let name = &v.ident;
        let fields = v.fields.clone().into_token_stream();
        match &v.fields {
            Fields::Named(_) => {
                quote! {
                    #[derive(Debug, serde::Serialize, serde::Deserialize)]
                    pub struct #name #fields
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    #[derive(Debug, serde::Serialize, serde::Deserialize)]
                    pub struct #name #fields;
                }
            }
            Fields::Unit => quote! {
                #[derive(Debug, serde::Serialize, serde::Deserialize)]
                pub struct #name;
            },
        }
    });

    quote! {
        #(#structs)*
    }
}

fn generate_impls(
    enum_name: &Ident,
    variants: &Punctuated<Variant, Comma>,
) -> proc_macro2::TokenStream {
    let impls = variants.iter().map(|v| {
        let name = &v.ident;
        quote! {
            impl crate::proto::Message for #name {
                fn msg_source(&self) -> MessageSource {
                    #enum_name::#name.into()
                }
            }
        }
    });

    quote! {
        #(#impls)*
    }
}
