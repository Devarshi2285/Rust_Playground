use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};
#[proc_macro_derive(MyDeSerialize)]
pub fn deserialize_derive(item: TokenStream) -> TokenStream {
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
        let ty = &f.ty;

        quote! {
            #ident : data_value_map
                .get(#key)
                .unwrap()
                .parse::<#ty>()
                .unwrap()
        }
    });

    let expanded = quote! {

        impl MyDeSerialize for #name {
            fn mydeserialize(data:&String)->User{

               let mut data_mut=data.clone();

               if data_mut.starts_with('{'){
                    data_mut.remove(0);
               }

                if data_mut.ends_with('}'){
                    data_mut.remove(data_mut.len()-1);
               }

               println!("{}",data_mut);

               let data_fields:Vec<String>=data_mut.split(',').map(|s| s.to_string()).collect();

               let mut data_value_map=std::collections::HashMap::new();

               for x in &data_fields{
                let mut parts = x.split(':');
                let mut key = parts.next().unwrap().trim();
                let mut value = parts.next().unwrap().trim();

                data_value_map.insert( key , value );
               }

               #name{
               #(#fields_name),*
               }

            }
        }

    };

    TokenStream::from(expanded)
}
