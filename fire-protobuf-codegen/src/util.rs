use crate::attr::FieldAttr;

use proc_macro2::{Span, TokenStream};
use syn::{
	Result, Error, Attribute, Ident, Expr, ExprLit, Lit, LitInt, Fields,
	Variant, TypePath, Field
};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use quote::quote;

use proc_macro_crate::{crate_name, FoundCrate};


pub(crate) fn fire_protobuf_crate() -> Result<TokenStream> {
	let name = crate_name("fire-protobuf")
		.map_err(|e| Error::new(Span::call_site(), e))?;

	Ok(match name {
		// if it get's used inside fire_protobuf it is a test or an example
		FoundCrate::Itself => quote!(fire_protobuf),
		FoundCrate::Name(n) => {
			let ident = Ident::new(&n, Span::call_site());
			quote!(#ident)
		}
	})
}

pub(crate) fn repr_as_i32(attrs: Vec<Attribute>) -> Result<bool> {
	let mut repr_as = None;

	for attr in attrs {
		if !attr.path.is_ident("repr") {
			continue
		}

		let ty: TypePath = attr.parse_args()?;

		repr_as = Some(ty);
	}

	match repr_as {
		Some(path) => {
			if !path.path.is_ident("i32") {
				return Err(Error::new_spanned(path, "expected i32"));
			}

			Ok(true)
		},
		None => Ok(false)
	}
}

// (variants, default)
pub(crate) fn variants_no_fields(
	variants: Punctuated<Variant, Comma>
) -> Result<(Vec<(LitInt, Ident)>, (LitInt, Ident))> {
	let mut variants: Vec<_> = variants.into_iter()
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

			let ident = v.ident;

			if !matches!(v.fields, Fields::Unit) {
				return Err(Error::new_spanned(v.fields, "no fields allowed"))
			}

			Ok((fieldnum, ident))
		})
		.collect::<Result<_>>()?;

	// get the default field
	let default_variant = variants.iter()
		.position(|(num, _)| num.base10_digits() == "0")
		.ok_or_else(|| {
			Error::new(Span::call_site(), "a fields needs to be the default")
		})?;
	let default_variant = variants.remove(default_variant);

	let has_still_default = variants.iter()
		.any(|(num, _)| num.base10_digits() == "0");

	if has_still_default {
		Err(Error::new(Span::call_site(), "only one variant can be default = \
			0"))
	} else {
		Ok((variants, default_variant))
	}
}

pub(crate) fn variants_with_fields(
	variants: Punctuated<Variant, Comma>
) -> Result<Vec<(FieldAttr, Ident, Option<Field>)>> {
	let v = variants.into_iter()
		.map(|v| {
			let attr = FieldAttr::from_attrs(&v.attrs)?;

			let ident = v.ident;

			let field = match v.fields {
				Fields::Unnamed(unnamed) => {
					if unnamed.unnamed.len() != 1 {
						return Err(Error::new_spanned(
							unnamed,
							"only one unnamed field allowed"
						));
					}

					let field = unnamed.unnamed.into_iter().next().unwrap();
					Some(field)
				},
				Fields::Unit => None,
				Fields::Named(n) => return Err(
					Error::new_spanned(n, "named fields not supported")
				)
			};

			Ok((attr, ident, field))
		})
		.collect::<Result<Vec<_>>>()?;

	let defaults = v.iter()
		.filter(|(attr, _, _)| attr.default.is_some())
		.count();

	if defaults == 0 {
		Err(Error::new(Span::call_site(), "one variant needs to the \
			default have #[field(num, default)]"))
	} else if defaults > 1 {
		Err(Error::new(Span::call_site(), "only one variant can be the \
			default"))
	} else {
		Ok(v)
	}
}