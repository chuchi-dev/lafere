mod encode;
mod decode;
mod attr;
mod util;

use proc_macro::TokenStream;

use syn::{parse_macro_input, DeriveInput};

const FIELD: &str = "field";

#[proc_macro_derive(EncodeMessage, attributes(field))]
pub fn derive_encode_message(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);

	encode::expand(input)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}

#[proc_macro_derive(DecodeMessage, attributes(field))]
pub fn derive_decode_message(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);

	decode::expand(input)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}