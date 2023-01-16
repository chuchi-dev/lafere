use crate::ApiArgs;
use crate::util::{
	validate_signature, fire_api_crate, validate_inputs, ref_type
};

use proc_macro2::{TokenStream};
use syn::{Result, ItemFn};
use quote::{quote, format_ident};


pub(crate) fn expand(
	args: ApiArgs,
	item: ItemFn
) -> Result<TokenStream> {
	let fire = fire_api_crate()?;
	let req_ty = args.ty;

	validate_signature(&item.sig)?;

	// Box<Type>
	let input_types = validate_inputs(item.sig.inputs.iter())?;

	let struct_name = &item.sig.ident;
	let struct_gen = generate_struct(&item);

	//
	let ty_as_req = quote!(<#req_ty as #fire::request::Request>);

	let type_action = quote!(
		type Action = #ty_as_req::Action;
	);

	let action_fn = quote!(
		fn action() -> Self::Action
		where Self: Sized {
			#ty_as_req::ACTION
		}
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
							::<#elem, #req_ty>
					)
				},
				None => quote!(
					#fire::util::valid_data_as_owned
						::<#ty, #req_ty>
				)
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

	let handler_fn = {
		let asyncness = &item.sig.asyncness;
		let inputs = &item.sig.inputs;
		let output = &item.sig.output;
		let block = &item.block;

		quote!(
			#asyncness fn handler( #inputs ) #output
				#block
		)
	};

	let handle_fn = {
		let is_async = item.sig.asyncness.is_some();
		let await_kw = if is_async {
			quote!(.await)
		} else {
			quote!()
		};

		let mut handler_args_vars = vec![];
		let mut handler_args = vec![];

		for (idx, ty) in input_types.iter().enumerate() {
			let get_fn = match ref_type(&ty) {
				Some(reff) => {
					let elem = &reff.elem;
					quote!(
						#fire::util::get_data_as_ref
							::<#elem, #req_ty>
					)
				},
				None => quote!(
					#fire::util::get_data_as_owned
						::<#ty, #req_ty>
				)
			};

			let var_name = format_ident!("handler_arg_{idx}");

			handler_args_vars.push(quote!(
				let #var_name = #get_fn(data, session, &req);
			));
			handler_args.push(quote!(#var_name));
		}

		let action_ty = quote!(#ty_as_req::Action);
		let msg_ty = quote!(#fire::message::Message<#action_ty, B>);
		let from_msg = quote!(#fire::message::FromMessage<#action_ty, B>);
		let into_msg = quote!(#fire::message::IntoMessage<#action_ty, B>);
		let api_err = quote!(#fire::error::ApiError);

		quote!(
			fn handle<'a>(
				&'a self,
				msg: #msg_ty,
				data: &'a #fire::server::Data,
				session: &'a #fire::server::Session
			) -> #fire::util::PinnedFuture<'a,
				std::result::Result<#msg_ty, #fire::error::Error>
			> {
				#handler_fn

				type __Response = #ty_as_req::Response;
				type __Error = #ty_as_req::Error;

				// msg to req
				// call handler
				// convert resp to msg
				// convert err to msg

				async fn handle_with_api_err<B>(
					msg: #msg_ty,
					data: &#fire::server::Data,
					session: &#fire::server::Session
				) -> std::result::Result<#msg_ty, __Error>
				where B: #fire::message::PacketBytes {

					let req = <#req_ty as #from_msg>::from_message(msg)
						.map_err(<__Error as #api_err>::from_message_error)?;

					let req = #fire::util::DataManager::new(req);

					#(#handler_args_vars)*

					let resp: __Response = handler(
						#(#handler_args),*
					)#await_kw?;

					<__Response as #into_msg>::into_message(resp)
						.map_err(<__Error as #api_err>::from_message_error)
				}

				#fire::util::PinnedFuture::new(async move {

					let r = handle_with_api_err(msg, data, session).await;

					match r {
						Ok(m) => Ok(m),
						Err(e) => {
							<__Error as #into_msg>::into_message(e)
								.map_err(#fire::error::Error::from)
						}
					}
				})
			}
		)
	};

	Ok(quote!(
		#struct_gen

		impl<B> #fire::request::RequestHandler<B> for #struct_name
		where B: #fire::message::PacketBytes + Send + 'static {
			#type_action
			#action_fn
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