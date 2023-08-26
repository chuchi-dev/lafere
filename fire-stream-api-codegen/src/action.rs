use crate::util::fire_api_crate;

use proc_macro2::TokenStream;
use syn::{
	Result, DeriveInput, Error, Ident, Attribute, Data, Expr, ExprLit, Lit,
	LitInt, Fields, TypePath, Variant
};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use quote::quote;


pub(crate) fn expand(input: DeriveInput) -> Result<TokenStream> {
	let DeriveInput { attrs, ident, generics, data, .. } = input;

	let d = match data {
		Data::Enum(e) => e,
		_ => return Err(Error::new_spanned(ident, "only enums supported"))
	};

	if !repr_as_u16(attrs)? {
		return Err(Error::new_spanned(ident, "#[repr(u16)] required"))
	}

	if !generics.params.is_empty() {
		return Err(Error::new_spanned(generics, "generics not supported"))
	}

	// (fieldnum, ident)
	let variants = variants_no_fields(d.variants)?;

	let fire = fire_api_crate()?;

	let from_variants = variants.iter()
		.map(|(num, id)| quote!(#num => Some(Self::#id)));

	let from_u16 = quote!(
		fn from_u16(num: u16) -> Option<Self> {
			match num {
				#(#from_variants),*,
				_ => None
			}
		}
	);

	let to_variants = variants.iter()
		.map(|(num, id)| quote!(Self::#id => #num));

	let as_u16 = quote!(
		fn as_u16(&self) -> u16 {
			match self {
				#(#to_variants),*
			}
		}
	);

	Ok(quote!(
		impl #fire::message::Action for #ident {
			#from_u16
			#as_u16
		}
	))
}

fn repr_as_u16(attrs: Vec<Attribute>) -> Result<bool> {
	let mut repr_as = None;

	for attr in attrs {
		if !attr.path().is_ident("repr") {
			continue
		}

		let ty: TypePath = attr.parse_args()?;

		repr_as = Some(ty);
	}

	match repr_as {
		Some(path) => {
			if !path.path.is_ident("u16") {
				return Err(Error::new_spanned(path, "expected u16"));
			}

			Ok(true)
		},
		None => Ok(false)
	}
}

fn variants_no_fields(
	variants: Punctuated<Variant, Comma>
) -> Result<Vec<(LitInt, Ident)>> {
	variants.into_iter()
		.map(|v| {
			let fieldnum_expr = v.discriminant
				.ok_or_else(|| Error::new_spanned(
					&v.ident,
					"needs to have a field number `Ident = x`"
				))?
				.1;
			let fieldnum = match fieldnum_expr {
				Expr::Lit(ExprLit { lit: Lit::Int(int), .. }) => int,
				e => return Err(Error::new_spanned(e, "expected = int"))
			};

			let fieldnum_zero = fieldnum.base10_digits() == "0";

			if fieldnum_zero {
				return Err(Error::new_spanned(
					fieldnum_zero,
					"zero not allowed"
				))
			}

			if !matches!(v.fields, Fields::Unit) {
				return Err(Error::new_spanned(v.fields, "no fields allowed"))
			}

			Ok((fieldnum, v.ident))
		})
		.collect()
}