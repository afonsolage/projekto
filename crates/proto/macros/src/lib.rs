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
    let des_boxed_match_items = variants.iter().map(|v| {
        let v_name = &v.ident;
        quote! {
            #name::#v_name => {
                let msg = projekto_proto::decode::<#v_name>(buf)?;
                Ok(Box::new(msg))
            }
        }
    });
    let ser_boxed_match_items = variants.iter().map(|v| {
        let v_name = &v.ident;
        quote! {
            #name::#v_name => {
                match boxed.downcast::<#v_name>() {
                    Ok(msg) => projekto_proto::encode(buf, &msg),
                    Err(boxed) => Err(projekto_proto::MessageError::Downcasting(boxed.msg_source()))
                }
            }
        }
    });

    let from_code_match_items = variants.iter().enumerate().map(|(i, v)| {
        let v_name = &v.ident;
        let i = i as u16;
        quote! {
            #i => Ok(#name::#v_name),
        }
    });

    let code_match_items = variants.iter().enumerate().map(|(i, v)| {
        let v_name = &v.ident;
        let i = i as u16;
        quote! {
            #name::#v_name => #i,
        }
    });

    let handlers_match_items = variants.iter().map(|v| {
        let v_name = &v.ident;
        let no_copy = v.attrs.iter().any(|attr| attr.path().is_ident("no_copy"));
        if no_copy {
            quote!{
                #name::#v_name => {
                    projekto_proto::RunMessageHandlers::run_handler::<#v_name>(world, client_id, boxed);
                },
            }
        } else {
            quote! {
                #name::#v_name => {
                    projekto_proto::RunMessageHandlers::run_handlers::<#v_name>(world, client_id, boxed);
                },
            }
        }
    });

    let is_unit_type = variants.iter().map(|v| {
        let v_name = &v.ident;
        let is_unit_type = matches!(v.fields, Fields::Unit);
        quote! {
            #name::#v_name => #is_unit_type,
        }
    });

    let variant_names = variants.iter().map(|v| &v.ident);
    quote! {
        #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
        pub enum #name {
            #(#variant_names),*
        }

        impl projekto_proto::MessageType for #name {
            fn source() -> MessageSource {
                #source
            }

            fn deserialize_boxed(&self, buf: &[u8]) -> Result<projekto_proto::BoxedMessage<Self>, projekto_proto::MessageError> {
                match self {
                    #(#des_boxed_match_items),*
                }
            }

            fn serialize_boxed(&self, boxed: projekto_proto::BoxedMessage<Self>, buf: &mut [u8]) -> Result<u32, projekto_proto::MessageError> {
                match self {
                    #(#ser_boxed_match_items),*
                }
            }

            fn try_from_code(n: u16) -> Result<Self, projekto_proto::MessageError> {
                match n {
                    #(#from_code_match_items)*
                    _ => Err(projekto_proto::MessageError::InvalidMessage(Self::name(), n)),
                }
            }

            fn code(&self) -> u16 {
                match self {
                    #(#code_match_items)*
                }
            }

            fn name() -> &'static str {
                stringify!(#name)
            }

            fn run_handlers(&self, boxed: projekto_proto::BoxedMessage<Self>, client_id: projekto_proto::ClientId, world: &mut bevy::prelude::World) {
                match self {
                    #(#handlers_match_items)*
                }
            }

            fn is_unit_type(&self) -> bool {
                match self {
                    #(#is_unit_type)*
                }
            }
        }
    }
}

fn generate_structs(variants: &Punctuated<Variant, Comma>) -> proc_macro2::TokenStream {
    let structs = variants.iter().map(|v| {
        let name = &v.ident;
        let fields = v.fields.clone().into_token_stream();
        let no_copy = v.attrs.iter().any(|attr| attr.path().is_ident("no_copy"));

        let copy_impl = if no_copy {
            quote! {
                impl projekto_proto::NoCopy for #name {}
            }
        } else {
            quote! {
                impl std::marker::Copy for #name {}
            }
        };

        match &v.fields {
            Fields::Named(_) => {
                quote! {
                    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                    pub struct #name #fields

                    #copy_impl
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                    pub struct #name #fields;

                    #copy_impl
                }
            }
            Fields::Unit => {
                if no_copy {
                    panic!("Unit variants are always Copy. Remove no_copy.");
                }
                quote! {
                    #[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
                    pub struct #name;
                }
            }
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
            impl projekto_proto::Message<#enum_name> for #name {
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
