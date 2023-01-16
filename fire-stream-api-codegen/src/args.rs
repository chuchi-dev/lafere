use syn::parse::{Parse, ParseStream, Result};

use syn::Type;


#[derive(Clone)]
pub(crate) struct ApiArgs {
	pub ty: Type
}

impl Parse for ApiArgs {
	fn parse(input: ParseStream) -> Result<Self> {
		let ty: Type = input.parse()?;

		Ok(Self { ty })
	}
}