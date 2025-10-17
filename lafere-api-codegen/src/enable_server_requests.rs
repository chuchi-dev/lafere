use crate::EnableServerRequestsArgs;
use crate::util::{
	lafere_api_crate, ref_type, validate_inputs, validate_signature,
};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, Result};

pub(crate) fn expand(
	args: EnableServerRequestsArgs,
	item: ItemFn,
) -> Result<TokenStream> {
	let fire = lafere_api_crate()?;
	let action_ty = args.ty;
	let use_generics = !args.bytes.is_some();
	let bytes_ty = match args.bytes {
		Some(b) => quote!(#b),
		None => quote!(B),
	};

	validate_signature(&item.sig)?;

	// Box<Type>
	let input_types = validate_inputs(item.sig.inputs.iter())?;

	let struct_name = &item.sig.ident;
	let struct_gen = generate_struct(&item);

	//
	let requestor_ty =
		quote!(#fire::requestor::Requestor<#action_ty, #bytes_ty>);

	let type_action = quote!(
		type Action = #action_ty;
	);

	let valid_data_fn = {
		let mut asserts = vec![];

		for ty in &input_types {
			let error_msg = format!("could not find {}", quote!(#ty));

			let valid_fn = match ref_type(&ty) {
				Some(reff) => {
					let elem = &reff.elem;
					quote!(
						#fire::util::valid_data_as_ref
							::<#elem, #requestor_ty>
					)
				}
				None => quote!(
					#fire::util::valid_data_as_owned
						::<#ty, #requestor_ty>
				),
			};

			asserts.push(quote!(
				assert!(#valid_fn(data), #error_msg);
			));
		}

		quote!(
			fn validate_data(&self, data: &#fire::server::Data) {
				#(#asserts)*
			}
		)
	};

	let generics_def = if use_generics {
		quote!(B: #fire::message::PacketBytes + Send + 'static)
	} else {
		quote!()
	};

	let handler_fn = {
		let asyncness = &item.sig.asyncness;
		let inputs = &item.sig.inputs;
		let output = &item.sig.output;
		let block = &item.block;

		quote!(
			#asyncness fn handler<#generics_def>(
				#inputs
			) #output
				#block
		)
	};

	let handle_fn = {
		let is_async = item.sig.asyncness.is_some();
		let await_kw = if is_async { quote!(.await) } else { quote!() };

		let mut handler_args_vars = vec![];
		let mut handler_args = vec![];

		for (idx, ty) in input_types.iter().enumerate() {
			let get_fn = match ref_type(&ty) {
				Some(reff) => {
					let elem = &reff.elem;
					quote!(
						#fire::util::get_data_as_ref
							::<#elem, #requestor_ty>
					)
				}
				None => quote!(
					#fire::util::get_data_as_owned
						::<#ty, #requestor_ty>
				),
			};

			let var_name = format_ident!("handler_arg_{idx}");

			handler_args_vars.push(quote!(
				let #var_name = #get_fn(data, session, &sender);
			));
			handler_args.push(quote!(#var_name));
		}

		let maybe_gen = if use_generics { quote!(B) } else { quote!() };

		quote!(
			fn handle<'a>(
				&'a self,
				sender: #requestor_ty,
				data: &'a #fire::server::Data,
				session: &'a #fire::server::Session
			) -> #fire::util::PinnedFuture<'a,
				std::result::Result<(), #fire::error::Error>
			> {
				#handler_fn

				#fire::util::PinnedFuture::new(async move {
					let sender = #fire::util::DataManager::new(sender);

					#(#handler_args_vars)*

					handler::<#maybe_gen>(
						#(#handler_args),*
					)#await_kw
						.map_err(Into::into)
				})
			}
		)
	};

	Ok(quote!(
		#struct_gen

		impl<#generics_def> #fire::request::EnableServerRequestsHandler<#bytes_ty> for #struct_name {
			#type_action
			#valid_data_fn
			#handle_fn
		}
	))
}

pub(crate) fn generate_struct(item: &ItemFn) -> TokenStream {
	let struct_name = &item.sig.ident;
	let attrs = &item.attrs;
	let vis = &item.vis;

	quote!(
		#(#attrs)*
		#[allow(non_camel_case_types)]
		#vis struct #struct_name;
	)
}
