use crate::util::{
	fire_protobuf_crate, variants_no_fields, repr_as_i32, variants_with_fields
};
use crate::attr::FieldAttr;

use proc_macro2::TokenStream;
use syn::{
	DeriveInput, Error, Attribute, Ident, Generics, Data, DataStruct, DataEnum,
	Fields
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

	if !generics.params.is_empty() {
		return Err(Error::new_spanned(generics, "generics not supported"))
	}

	// parse fields
	let fields: Vec<_> = fields.into_iter()
		.map(|f| Ok((FieldAttr::from_attrs(&f.attrs)?, f)))
		.collect::<Result<_, Error>>()?;

	let fire = fire_protobuf_crate()?;
	let fire_decode = quote!(#fire::decode);

	// the wire type for structs is always len
	let wire_type = quote!(#fire::WireType::Len);
	let wire_type_const = quote!(
		const WIRE_TYPE: #fire::WireType = #wire_type;
	);

	let dectrait = quote!(#fire_decode::DecodeMessage);

	let merge_fields: Vec<_> = fields.iter()
		.map(|(attr, f)| {
			let id = &f.ident;
			let fieldnum = &attr.fieldnum;
			quote!(
				#fieldnum => #dectrait::merge(&mut self.#id, field.kind, true)?
			)
		})
		.collect();


	let merge = quote!(
		fn merge(
			&mut self,
			kind: #fire_decode::FieldKind<'m>,
			_is_field: bool
		) -> Result<(), #fire_decode::DecodeError> {
			let mut parser = #fire_decode::MessageDecoder::try_from_kind(kind)?;

			while let Some(field) = parser.next()? {
				match field.number {
					#(
						#merge_fields
					),*,
					// ignore unknown fields
					_ => {}
				}
			}

			parser.finish()
		}
	);


	Ok(quote!(
		impl<'m> #dectrait<'m> for #ident {
			#wire_type_const
			#merge
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

/// only call this if the type is repr(i32)
fn expand_enum_no_fields(
	ident: Ident,
	generics: Generics,
	d: DataEnum
) -> Result<TokenStream, Error> {
	if !generics.params.is_empty() {
		return Err(Error::new_spanned(generics, "generics not supported"))
	}

	// (fieldnum, ident)
	let (variants, default_variant) = variants_no_fields(d.variants)?;
	let default_variant = default_variant.1;


	let fire = fire_protobuf_crate()?;
	let fire_decode = quote!(#fire::decode);

	// the wire type for structs is always len
	let wire_type = quote!(#fire::WireType::Varint);
	let wire_type_const = quote!(
		const WIRE_TYPE: #fire::WireType = #wire_type;
	);

	let dectrait = quote!(#fire_decode::DecodeMessage);

	let merge_variants: Vec<_> = variants.iter()
		.map(|(num, id)| quote!(#num => Self::#id))
		.collect();

	let merge = quote!(
		fn merge(
			&mut self,
			kind: #fire_decode::FieldKind<'m>,
			_is_field: bool
		) -> Result<(), #fire_decode::DecodeError> {
			let num = kind.try_unwrap_varint()?;

			*self = match num {
				#(
					#merge_variants
				),*,
				_ => Self::#default_variant
			};

			Ok(())
		}
	);

	Ok(quote!(
		impl<'m> #dectrait<'m> for #ident {
			#wire_type_const
			#merge
		}
	))
}

/// only call this if the enum has no #[repr(..)] attribute
fn expand_enum_with_fields(
	ident: Ident,
	generics: Generics,
	d: DataEnum
) -> Result<TokenStream, Error> {
	if !generics.params.is_empty() {
		return Err(Error::new_spanned(generics, "generics not supported"))
	}

	// (FieldAttr, ident, Option<field>)
	let variants = variants_with_fields(d.variants)?;

	let fire = fire_protobuf_crate()?;
	let fire_decode = quote!(#fire::decode);

	// the wire type for structs is always len
	let wire_type = quote!(#fire::WireType::Len);
	let wire_type_const = quote!(
		const WIRE_TYPE: #fire::WireType = #wire_type;
	);

	let dectrait = quote!(#fire_decode::DecodeMessage);

	let match_fields: Vec<_> = variants.iter()
		.map(|(attr, ident, field)| {
			let fieldnum = &attr.fieldnum;

			if let Some(field) = field {
				quote!(#fieldnum => {
					match self {
						Self::#ident(v) => {
							#dectrait::merge(v, field.kind, true)?
						},
						_ => {
							let mut v = <#field as std::default::Default>
								::default();

							#dectrait::merge(&mut v, field.kind, true)?;
							*self = Self::#ident(v);
						}
					}
				})
			} else {
				quote!(#fieldnum => *self = Self::#ident)
			}
		})
		.collect();

	let merge = quote!(
		fn merge(
			&mut self,
			kind: #fire_decode::FieldKind<'m>,
			_is_field: bool
		) -> Result<(), #fire_decode::DecodeError> {
			let mut parser = #fire_decode::MessageDecoder::try_from_kind(kind)?;

			while let Some(field) = parser.next()? {
				match field.number {
					#(#match_fields),*,
					_ => {}
				}
			}

			parser.finish()
		}
	);

	Ok(quote!(
		impl<'m> #dectrait<'m> for #ident {
			#wire_type_const
			#merge
		}
	))
}