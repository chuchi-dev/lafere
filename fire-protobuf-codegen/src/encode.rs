use crate::util::{
	fire_protobuf_crate, repr_as_i32, variants_no_fields, variants_with_fields
};
use crate::attr::FieldAttr;

use std::iter;

use proc_macro2::TokenStream;
use syn::{
	DeriveInput, Error, Ident, Generics, Data, DataStruct, DataEnum,
	Fields, Attribute
};
use quote::quote;


pub(crate) fn expand(input: DeriveInput) -> Result<TokenStream, Error> {
	let DeriveInput { attrs, ident, generics, data, .. } = input;

	match data {
		Data::Struct(d) => expand_struct(ident, generics, d),
		Data::Enum(e) => expand_enum(attrs, ident, generics, e),
		Data::Union(_) => Err(Error::new(ident.span(), "union not supported"))
	}
}


fn expand_struct(
	ident: Ident,
	generics: Generics,
	d: DataStruct
) -> Result<TokenStream, Error> {
	let fields = match d.fields {
		Fields::Named(f) => f.named,
		_ => return Err(Error::new(ident.span(), "only named structs"))
	};

	// parse fields
	let fields: Vec<_> = fields.into_iter()
		.map(|f| Ok((FieldAttr::from_attrs(&f.attrs)?, f)))
		.collect::<Result<_, Error>>()?;

	let fire = fire_protobuf_crate()?;
	let fire_encode = quote!(#fire::encode);

	// the wire type for structs is always len
	let wire_type = quote!(#fire::WireType::Len);
	let wire_type_const = quote!(
		const WIRE_TYPE: #fire::WireType = #wire_type;
	);

	let enctrait = quote!(#fire_encode::EncodeMessage);

	let encoded_size_fields: Vec<_> = fields.iter()
		.map(|(attr, f)| {
			let id = &f.ident;
			let fieldnum = &attr.fieldnum;
			quote!(
				if !#enctrait::is_default(&self.#id) {
					#enctrait::encoded_size(
						&mut self.#id,
						Some(#fire_encode::FieldOpt::new(#fieldnum)),
						&mut size
					)?;
				}
			)
		})
		.collect();

	let encoded_size = quote!(
		fn encoded_size(
			&mut self,
			field: Option<#fire_encode::FieldOpt>,
			builder: &mut #fire_encode::SizeBuilder
		) -> Result<(), #fire_encode::EncodeError> {
			let mut size = #fire_encode::SizeBuilder::new();
			#(
				#encoded_size_fields
			)*
			let fields_size = size.finish();

			if let Some(field) = field {
				builder.write_tag(field.num, #wire_type);
				builder.write_len(fields_size);
			}

			builder.write_bytes(fields_size);

			Ok(())
		}
	);


	let encode_fields: Vec<_> = fields.iter()
		.map(|(attr, f)| {
			let id = &f.ident;
			let fieldnum = &attr.fieldnum;
			quote!(
				if !#enctrait::is_default(&self.#id) {
					#enctrait::encode(
						&mut self.#id,
						Some(#fire_encode::FieldOpt::new(#fieldnum)),
						encoder
					)?;
				}
			)
		})
		.collect();


	let encode = quote!(
		fn encode<B>(
			&mut self,
			field: Option<#fire_encode::FieldOpt>,
			encoder: &mut #fire_encode::MessageEncoder<B>
		) -> Result<(), #fire_encode::EncodeError>
		where B: #fire::bytes::BytesWrite {
			#[cfg(debug_assertions)]
			let mut dbg_fields_size = None;

			// we don't need to get the size if we don't need to write
			// the size
			if let Some(field) = field {
				encoder.write_tag(field.num, #wire_type)?;

				let mut size = #fire_encode::SizeBuilder::new();
				#(
					#encoded_size_fields
				)*
				let fields_size = size.finish();

				encoder.write_len(fields_size)?;

				#[cfg(debug_assertions)]
				{
					dbg_fields_size = Some(fields_size);
				}
			}

			#[cfg(debug_assertions)]
			let prev_len = encoder.written_len();

			#(
				#encode_fields
			)*

			#[cfg(debug_assertions)]
			if let Some(fields_size) = dbg_fields_size {
				let added_len = encoder.written_len() - prev_len;
				assert_eq!(fields_size, added_len as u64,
					"encoded size does not match actual size");
			}

			Ok(())
		}
	);

	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();


	Ok(quote!(
		impl #impl_generics #enctrait for #ident #ty_generics #where_clause {
			#wire_type_const
			fn is_default(&self) -> bool { false }
			#encoded_size
			#encode
		}
	))
}

fn expand_enum(
	attrs: Vec<Attribute>,
	ident: Ident,
	generics: Generics,
	d: DataEnum
) -> Result<TokenStream, Error> {
	let repr_as_i32 = repr_as_i32(attrs)?;

	if repr_as_i32 {
		expand_enum_no_fields(ident, generics, d)
	} else {
		expand_enum_with_fields(ident, generics, d)
	}
}

fn expand_enum_no_fields(
	ident: Ident,
	generics: Generics,
	d: DataEnum
) -> Result<TokenStream, Error> {
	// (fieldnum, ident)
	let (variants, default_variant) = variants_no_fields(d.variants)?;

	let fire = fire_protobuf_crate()?;
	let fire_encode = quote!(#fire::encode);

	// the wire type for structs is always len
	let wire_type = quote!(#fire::WireType::Varint);
	let wire_type_const = quote!(
		const WIRE_TYPE: #fire::WireType = #wire_type;
	);

	let enctrait = quote!(#fire_encode::EncodeMessage);

	let match_variants: Vec<_> = variants.iter()
		.chain(iter::once(&default_variant))
		.map(|(num, ident)| quote!(Self::#ident => #num))
		.collect();

	let default_ident = default_variant.1;

	let is_default = quote!(
		fn is_default(&self) -> bool {
			matches!(self, Self::#default_ident)
		}
	);

	let encoded_size = quote!(
		fn encoded_size(
			&mut self,
			field: Option<#fire_encode::FieldOpt>,
			builder: &mut #fire_encode::SizeBuilder
		) -> Result<(), #fire_encode::EncodeError> {
			if let Some(field) = field {
				builder.write_tag(field.num, #wire_type);
			}

			let varint: i32 = match self {
				#(#match_variants),*
			};

			builder.write_varint(varint as u64);

			Ok(())
		}
	);

	let encode = quote!(
		fn encode<B>(
			&mut self,
			field: Option<#fire_encode::FieldOpt>,
			encoder: &mut #fire_encode::MessageEncoder<B>
		) -> Result<(), #fire_encode::EncodeError>
		where B: #fire::bytes::BytesWrite {
			if let Some(field) = field {
				encoder.write_tag(field.num, #wire_type)?;
			}

			let varint: i32 = match self {
				#(#match_variants),*
			};

			encoder.write_varint(varint as u64)
		}
	);

	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

	Ok(quote!(
		impl #impl_generics #enctrait for #ident #ty_generics #where_clause {
			#wire_type_const
			#is_default
			#encoded_size
			#encode
		}
	))
}

fn expand_enum_with_fields(
	ident: Ident,
	generics: Generics,
	d: DataEnum
) -> Result<TokenStream, Error> {
	// (FieldAttr, ident, Option<field>)
	let variants = variants_with_fields(d.variants)?;

	let fire = fire_protobuf_crate()?;
	let fire_encode = quote!(#fire::encode);

	// the wire type for structs is always len
	let wire_type = quote!(#fire::WireType::Len);
	let wire_type_const = quote!(
		const WIRE_TYPE: #fire::WireType = #wire_type;
	);

	let enctrait = quote!(#fire_encode::EncodeMessage);

	let encoded_size_variants: Vec<_> = variants.iter()
		.map(|(attr, ident, field)| {
			let fieldnum = &attr.fieldnum;

			if let Some(_) = field {
				quote!(
					Self::#ident(v) => {
						#enctrait::encoded_size(
							v,
							Some(#fire_encode::FieldOpt::new(#fieldnum)),
							&mut size
						)?
					}
				)
			} else {
				quote!(
					Self::#ident => {
						size.write_empty_field(#fieldnum)
					}
				)
			}
		})
		.collect();

	let encoded_size = quote!(
		fn encoded_size(
			&mut self,
			field: Option<#fire_encode::FieldOpt>,
			builder: &mut #fire_encode::SizeBuilder
		) -> Result<(), #fire_encode::EncodeError> {
			let mut size = #fire_encode::SizeBuilder::new();
			match self {
				#(#encoded_size_variants),*
			}
			let size = size.finish();

			if let Some(field) = field {
				builder.write_tag(field.num, #wire_type);
				builder.write_len(size);
			}

			builder.write_bytes(size);

			Ok(())
		}
	);

	let encode_variants: Vec<_> = variants.iter()
		.map(|(attr, ident, field)| {
			let fieldnum = &attr.fieldnum;

			if let Some(_) = field {
				quote!(
					Self::#ident(v) => {
						#enctrait::encode(
							v,
							Some(#fire_encode::FieldOpt::new(#fieldnum)),
							encoder
						)?
					}
				)
			} else {
				quote!(
					Self::#ident => {
						encoder.write_empty_field(#fieldnum)?
					}
				)
			}
		})
		.collect();

	let encode = quote!(
		fn encode<B>(
			&mut self,
			field: Option<#fire_encode::FieldOpt>,
			encoder: &mut #fire_encode::MessageEncoder<B>
		) -> Result<(), #fire_encode::EncodeError>
		where B: #fire::bytes::BytesWrite {
			#[cfg(debug_assertions)]
			let mut dbg_fields_size = None;

			/// we don't need to get the size if we don't need to write
			/// the size
			if let Some(field) = field {
				encoder.write_tag(field.num, #wire_type)?;

				let mut size = #fire_encode::SizeBuilder::new();
				match self {
					#(
						#encoded_size_variants
					)*
				}
				let fields_size = size.finish();

				encoder.write_len(fields_size)?;

				#[cfg(debug_assertions)]
				{
					dbg_fields_size = Some(fields_size);
				}
			}

			#[cfg(debug_assertions)]
			let prev_len = encoder.written_len();

			match self {
				#(
					#encode_variants
				)*
			}

			#[cfg(debug_assertions)]
			if let Some(fields_size) = dbg_fields_size {
				let added_len = encoder.written_len() - prev_len;
				assert_eq!(fields_size, added_len as u64,
					"encoded size does not match actual size");
			}

			Ok(())
		}
	);

	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

	Ok(quote!(
		impl #impl_generics #enctrait for #ident #ty_generics #where_clause {
			#wire_type_const
			fn is_default(&self) -> bool { false }
			#encoded_size
			#encode
		}
	))
}