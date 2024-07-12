use crate::FIELD;

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, LitInt, Path, Token};

pub struct FieldAttr {
	pub fieldnum: LitInt,
	pub default: Option<Path>,
}

impl FieldAttr {
	pub fn from_attrs(attrs: &[Attribute]) -> Result<Self, Error> {
		for attr in attrs {
			if !attr.path().is_ident(FIELD) {
				continue;
			}

			return attr.parse_args();
		}

		Err(Error::new(Span::call_site(), "expected #[field(..)]"))
	}
}

impl Parse for FieldAttr {
	fn parse(input: ParseStream) -> Result<Self, Error> {
		let fieldnum: LitInt = input.parse()?;
		let mut default = None;
		if input.lookahead1().peek(Token![,]) {
			input.parse::<Token![,]>()?;

			default = Some(input.parse()?);
		}

		if fieldnum.base10_digits() == "0" {
			return Err(Error::new_spanned(fieldnum, "numbers need to be > 0"));
		}

		Ok(Self { fieldnum, default })
	}
}
