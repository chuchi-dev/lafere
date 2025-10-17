use syn::parse::{Parse, ParseStream, Result};

use syn::punctuated::Punctuated;
use syn::{Token, Type};

#[derive(Clone)]
pub(crate) struct ApiArgs {
	pub ty: Type,
}

impl Parse for ApiArgs {
	fn parse(input: ParseStream) -> Result<Self> {
		let ty: Type = input.parse()?;

		Ok(Self { ty })
	}
}

#[derive(Clone)]
pub(crate) struct EnableServerRequestsArgs {
	pub ty: Type,
	pub bytes: Option<Type>,
}

impl Parse for EnableServerRequestsArgs {
	fn parse(input: ParseStream) -> Result<Self> {
		let list: Punctuated<Type, Token![,]> =
			Punctuated::parse_separated_nonempty(input)?;
		let mut iter = list.into_iter();

		Ok(Self {
			ty: iter.next().unwrap(),
			bytes: iter.next(),
		})
	}
}
