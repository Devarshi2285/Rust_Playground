use wasm_bindgen::prelude::*;


#[wasm_bindgen]
pub fn hello_from_rust()->String{
    return ("Hello").to_string();
}