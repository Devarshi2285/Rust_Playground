use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};
#[proc_macro_derive(MySerialize)]
pub fn serialize_derive(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let name = &input.ident;

    let fields = if let syn::Data::Struct(data) = input.data {
        if let syn::Fields::Named(fields) = data.fields {
            fields.named
        } else {
            panic!("Only named fields supported");
        }
    } else {
        panic!("Only structs are supported");
    };

    let fields_name = fields.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let key = ident.to_string();

        quote! {
            {
            let mut entry = String::new();
            entry.push_str(#key);
            entry.push_str(":");
            entry.push_str(&self.#ident.to_string());
            entry
            }
        }
    });

    let expanded = quote! {

        impl MySerialize for #name {
            fn serialize(&self)->String{

              let mut s = String::new();
                s.push_str("{");
                let mut parts = Vec::new();
                #(
                    parts.push(#fields_name);
                )*
                s.push_str(&parts.join(","));

                s.push_str("}");
                s
            }
        }

    };

    TokenStream::from(expanded)
}
