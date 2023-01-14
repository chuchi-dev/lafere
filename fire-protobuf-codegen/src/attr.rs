use crate::FIELD;

use proc_macro2::Span;
use syn::{Error, Attribute, Meta, NestedMeta, Lit, LitInt};


pub struct FieldAttr {
	pub fieldnum: LitInt
}

impl FieldAttr {
	pub fn from_attrs(attrs: &[Attribute]) -> Result<Self, Error> {
		let mut fieldnum = None;

		for attr in attrs {
			if !attr.path.is_ident(FIELD) {
				continue
			}

			let list = match attr.parse_meta() {
				Ok(Meta::List(list)) => list,
				Ok(other) => return Err(
					Error::new_spanned(other, "expected #[field(..)]")
				),
				Err(e) => return Err(e)
			};

			let first = list.nested.first().ok_or_else(|| {
					Error::new_spanned(list.path, "expected field number first")
				})?;

			match first {
				NestedMeta::Lit(Lit::Int(i)) => {
					fieldnum = Some(i.clone());
				},
				e => return Err(
					Error::new_spanned(e, "expected the field number")
				)
			}
		}

		let Some(fieldnum) = fieldnum else {
			return Err(Error::new(Span::call_site(), "expected #[field(..)]"))
		};

		if fieldnum.base10_digits() == "0" {
			return Err(Error::new_spanned(fieldnum, "numbers need to be > 0"))
		}

		Ok(Self { fieldnum })
	}
}