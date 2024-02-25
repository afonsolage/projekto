use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Comma, Data, DataEnum, DeriveInput, Fields,
    Path, Variant,
};

#[proc_macro_attribute]
pub fn message_source(attr: TokenStream, item: TokenStream) -> TokenStream {
    if attr.is_empty() {
        panic!("You must provide the MessageSource variant");
    }

    let source = parse_macro_input!(attr as Path);

    let ast = parse_macro_input!(item as DeriveInput);
    let Data::Enum(DataEnum { variants, .. }) = &ast.data else {
        panic!("message_source proc macro can only be used on enums");
    };
    let name = &ast.ident;

    let simplified_enum = generate_simplified_enum(name, &source, variants);
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
    source: &Path,
    variants: &Punctuated<Variant, Comma>,
) -> proc_macro2::TokenStream {
    let variant_names = variants.iter().map(|v| &v.ident);
    quote! {
        #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
        pub enum #name {
            #(#variant_names),*
        }

        impl crate::proto::MessageType for #name {
            fn source() -> MessageSource {
                #source
            }
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
            impl crate::proto::Message<#enum_name> for #name {
                fn msg_type(&self) -> #enum_name {
                    #enum_name::#name
                }
            }
        }
    });

    quote! {
        #(#impls)*
    }
}
