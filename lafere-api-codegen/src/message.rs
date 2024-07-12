use crate::util::lafere_api_crate;

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
	Attribute, DeriveInput, Error, Generics, Ident, TypeGenerics, WhereClause,
};

pub(crate) fn into_expand(input: DeriveInput) -> Result<TokenStream, Error> {
	let DeriveInput {
		attrs,
		ident,
		generics,
		..
	} = input;

	let attr = MsgAttribute::from_attrs(&attrs)?;
	let encdec_module = attr.module;

	let fire = lafere_api_crate()?;
	let message = quote!(#fire::message);

	let (impl_generics, ty_generics, where_clause) =
		split_generics(&generics, &fire);

	Ok(quote!(
		impl #impl_generics #message::IntoMessage<A, B> for #ident #ty_generics
		#where_clause {
			fn into_message(
				self
			) -> std::result::Result<
				#message::Message<A, B>,
				#fire::error::MessageError
			> {
				#fire::encdec::#encdec_module::encode(self)
			}
		}
	))
}

pub(crate) fn from_expand(input: DeriveInput) -> Result<TokenStream, Error> {
	let DeriveInput {
		attrs,
		ident,
		generics,
		..
	} = input;

	let attr = MsgAttribute::from_attrs(&attrs)?;
	let encdec_module = attr.module;

	let fire = lafere_api_crate()?;
	let message = quote!(#fire::message);

	let (impl_generics, ty_generics, where_clause) =
		split_generics(&generics, &fire);

	Ok(quote!(
		impl #impl_generics #message::FromMessage<A, B> for #ident #ty_generics
		#where_clause {
			fn from_message(
				msg: #message::Message<A, B>
			) -> std::result::Result<Self, #fire::error::MessageError> {
				#fire::encdec::#encdec_module::decode(msg)
			}
		}
	))
}

struct MsgAttribute {
	/// which module should be used to convert the types
	pub module: Ident,
}

impl MsgAttribute {
	pub fn from_attrs(attrs: &[Attribute]) -> Result<Self, Error> {
		let mut module = None;

		for attr in attrs {
			if !attr.path().is_ident("message") {
				continue;
			}

			module = Some(attr.parse_args()?);
		}

		Ok(Self {
			module: module.ok_or_else(|| {
				Error::new(
					Span::call_site(),
					"need an attribute #[message(..)]",
				)
			})?,
		})
	}
}

fn split_generics<'a>(
	generics: &'a Generics,
	fire: &TokenStream,
) -> (ImplGenerics, TypeGenerics<'a>, Option<&'a WhereClause>) {
	let mut impl_generics = generics.clone();
	impl_generics
		.params
		.push(syn::parse2(quote!(A: #fire::message::Action)).unwrap());
	impl_generics
		.params
		.push(syn::parse2(quote!(B: #fire::message::PacketBytes)).unwrap());

	let (_, ty_generics, where_clause) = generics.split_for_impl();
	(ImplGenerics(impl_generics), ty_generics, where_clause)
}

struct ImplGenerics(Generics);

impl ToTokens for ImplGenerics {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let (impl_generics, _, _) = self.0.split_for_impl();
		impl_generics.to_tokens(tokens);
	}
}
