use crate::FIELD;

use proc_macro2::Span;
use syn::{Error, Attribute, Meta, NestedMeta, Path, Lit, LitInt};


pub struct FieldAttr {
	pub fieldnum: LitInt,
	pub default: Option<Path>
}

impl FieldAttr {
	pub fn from_attrs(attrs: &[Attribute]) -> Result<Self, Error> {
		let mut fieldnum = None;
		let mut default = None;

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

			let mut nested = list.nested.into_iter();

			let first = nested.next().ok_or_else(|| {
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

			if let Some(second) = nested.next() {
				match second {
					NestedMeta::Meta(Meta::Path(p)) if p.is_ident("default") => {
						default = Some(p);
					},
					e => return Err(
						Error::new_spanned(e, "expected default")
					)
				}
			}
		}

		let fieldnum = match fieldnum {
			Some(f) => f,
			None => return Err(Error::new(
				Span::call_site(),
				"expected #[field(..)]"
			))
		};

		if fieldnum.base10_digits() == "0" {
			return Err(Error::new_spanned(fieldnum, "numbers need to be > 0"))
		}

		Ok(Self { fieldnum, default })
	}
}