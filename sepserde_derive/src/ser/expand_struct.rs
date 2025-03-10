use crate::common::{Field, YaSerdeAttribute, YaSerdeField};

use crate::ser::{element::*, implement_serializer::implement_serializer};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;
use syn::{DataStruct, Generics};

pub fn serialize(
    data_struct: &DataStruct,
    name: &Ident,
    root: &str,
    root_attributes: &YaSerdeAttribute,
    generics: &Generics,
) -> TokenStream {
    let append_attributes: TokenStream = data_struct
        .fields
        .iter()
        .map(|field| YaSerdeField::new(field.clone()))
        .filter(|field| field.is_attribute() || field.is_flatten())
        .map(|field| {
            let label = field.label();

            if field.is_attribute() {
                let label_name = field.renamed_label(root_attributes);

                match field.get_type() {
          Field::String
          | Field::Bool
          | Field::I8
          | Field::U8
          | Field::I16
          | Field::U16
          | Field::I32
          | Field::U32
          | Field::I64
          | Field::U64
          | Field::F32
          | Field::F64 => field.ser_wrap_default_attribute(
            Some(quote!(self.#label.to_string())),
            quote!({
              struct_start_event.attr(#label_name, &yaserde_inner)
            }),
          ),
          Field::Option { data_type } => match *data_type {
            Field::String => field.ser_wrap_default_attribute(
              None,
              quote!({
                if let ::std::option::Option::Some(ref value) = self.#label {
                  struct_start_event.attr(#label_name, value)
                } else {
                  struct_start_event
                }
              }),
            ),
            Field::Bool
            | Field::I8
            | Field::U8
            | Field::I16
            | Field::U16
            | Field::I32
            | Field::U32
            | Field::I64
            | Field::U64
            | Field::F32
            | Field::F64 => field.ser_wrap_default_attribute(
              Some(
                quote!(self.#label.map_or_else(|| ::std::string::String::new(), |v| v.to_string())),
              ),
              quote!({
                if let ::std::option::Option::Some(ref value) = self.#label {
                  struct_start_event.attr(#label_name, &yaserde_inner)
                } else {
                  struct_start_event
                }
              }),
            ),
            Field::Vec { .. } => {
              let item_ident = Ident::new("yaserde_item", field.get_span());
              let inner = enclose_formatted_characters(&item_ident, label_name);

              field.ser_wrap_default_attribute(
                None,
                quote!({
                  if let ::std::option::Option::Some(ref yaserde_list) = self.#label {
                    for yaserde_item in yaserde_list.iter() {
                      #inner
                    }
                  }
                }),
              )
            }
            Field::Struct { .. } => field.ser_wrap_default_attribute(
              Some(quote! {
              self.#label
                .as_ref()
                .map_or_else(
                  || ::std::result::Result::Ok(::std::string::String::new()),
                  |v| ::sepserde::ser::to_string_content(v),
                )?
              }),
              quote!({
                if let ::std::option::Option::Some(ref yaserde_struct) = self.#label {
                  struct_start_event.attr(#label_name, &yaserde_inner)
                } else {
                  struct_start_event
                }
              }),
            ),
            Field::Option { .. } => unimplemented!(),
          },
          Field::Struct { .. } => field.ser_wrap_default_attribute(
            Some(quote! { ::sepserde::ser::to_string_content(&self.#label)? }),
            quote!({
              struct_start_event.attr(#label_name, &yaserde_inner)
            }),
          ),
          Field::Vec { .. } => {
            // TODO
            quote!()
          }
        }
            } else {
                match field.get_type() {
                    Field::Struct { .. } => {
                        quote!(
                          let (attributes, namespace) = self.#label.serialize_attributes(
                            ::std::vec![],
                            ::sepserde::xml::namespace::Namespace::empty(),
                          )?;
                          child_attributes_namespace.extend(&namespace);
                          child_attributes.extend(attributes);
                        )
                    }
                    _ => quote!(),
                }
            }
        })
        .collect();

    let struct_inspector: TokenStream = data_struct
    .fields
    .iter()
    .map(|field| YaSerdeField::new(field.clone()))
    .filter(|field| !field.is_attribute() && !field.is_skip_serializing())
    .filter_map(|field| {
      let label = field.label();
      if field.is_text_content() {
        return match field.get_type() {
          Field::Option { .. } => Some(quote!(
            let s = self.#label.as_deref().unwrap_or_default();
            let data_event = ::sepserde::xml::writer::XmlEvent::characters(s);
            writer.write(data_event).map_err(|e| e.to_string())?;
          )),
          _ => Some(quote!(
            let data_event = ::sepserde::xml::writer::XmlEvent::characters(&self.#label);
            writer.write(data_event).map_err(|e| e.to_string())?;
          )),
        };
      }

      let label_name = field.renamed_label(root_attributes);
      let conditions = condition_generator(&label, &field);
      let generic = field.is_generic();


      match field.get_type() {
        Field::String
        | Field::Bool
        | Field::I8
        | Field::U8
        | Field::I16
        | Field::U16
        | Field::I32
        | Field::U32
        | Field::I64
        | Field::U64
        | Field::F32
        | Field::F64 => serialize_element(&label, label_name, &conditions),

        Field::Option { data_type } => match *data_type {
          Field::String
          | Field::Bool
          | Field::I8
          | Field::U8
          | Field::I16
          | Field::U16
          | Field::I32
          | Field::U32
          | Field::I64
          | Field::U64
          | Field::F32
          | Field::F64 => {
            let item_ident = Ident::new("yaserde_item", field.get_span());
            let inner = enclose_formatted_characters_for_value(&item_ident, label_name);

            Some(quote! {
              #conditions {
                if let Some(ref yaserde_item) = self.#label {
                  #inner
                }
              }
            })
          }
          Field::Vec { .. } => {
            let item_ident = Ident::new("yaserde_item", field.get_span());
            let inner = enclose_formatted_characters_for_value(&item_ident, label_name);

            Some(quote! {
              #conditions {
                if let ::std::option::Option::Some(ref yaserde_items) = &self.#label {
                  for yaserde_item in yaserde_items.iter() {
                    #inner
                  }
                }
              }
            })
          }
          Field::Struct { .. } => Some(if field.is_flatten() {
            quote! {
              if let ::std::option::Option::Some(ref item) = &self.#label {
                writer.set_start_event_name(::std::option::Option::None);
                writer.set_skip_start_end(true);
                writer.set_generic(#generic);
                ::sepserde::YaSerialize::serialize(item, writer)?;
              }
            }
          } else {
            quote! {
              if let ::std::option::Option::Some(ref item) = &self.#label {
                writer.set_start_event_name(::std::option::Option::Some(#label_name.to_string()));
                writer.set_skip_start_end(false);
                writer.set_generic(#generic);
                ::sepserde::YaSerialize::serialize(item, writer)?;
              }
            }
          }),
          _ => unimplemented!(),
        },
        Field::Struct { .. } => {
          let (start_event, skip_start) = if field.is_flatten() {
            (quote!(::std::option::Option::None), true)
          } else {
            (
              quote!(::std::option::Option::Some(#label_name.to_string())),
              false,
            )
          };

          Some(quote! {
            writer.set_start_event_name(#start_event);
            writer.set_skip_start_end(#skip_start);
            writer.set_generic(#generic);
            ::sepserde::YaSerialize::serialize(&self.#label, writer)?;
          })
        }
        Field::Vec { data_type } => match *data_type {
          Field::String => {
            let item_ident = Ident::new("yaserde_item", field.get_span());
            let inner = enclose_formatted_characters_for_value(&item_ident, label_name);

            Some(quote! {
              for yaserde_item in &self.#label {
                #inner
              }
            })
          }
          Field::Bool
          | Field::I8
          | Field::U8
          | Field::I16
          | Field::U16
          | Field::I32
          | Field::U32
          | Field::I64
          | Field::U64
          | Field::F32
          | Field::F64 => {
            let item_ident = Ident::new("yaserde_item", field.get_span());
            let inner = enclose_formatted_characters_for_value(&item_ident, label_name);

            Some(quote! {
              for yaserde_item in &self.#label {
                #inner
              }
            })
          }
          Field::Option { .. } => Some(quote! {
            for item in &self.#label {
              if let Some(value) = item {
                writer.set_start_event_name(None);
                writer.set_skip_start_end(false);
                writer.set_generic(#generic);
                ::sepserde::YaSerialize::serialize(value, writer)?;
              }
            }
          }),
          Field::Struct { .. } => {
            if field.is_flatten() {
              Some(quote! {
                for item in &self.#label {
                    writer.set_start_event_name(::std::option::Option::None);
                  writer.set_skip_start_end(true);
                  writer.set_generic(#generic);
                  ::sepserde::YaSerialize::serialize(item, writer)?;
                }
              })
            } else {
              Some(quote! {
                for item in &self.#label {
                  writer.set_start_event_name(::std::option::Option::Some(#label_name.to_string()));
                  writer.set_skip_start_end(false);
                  writer.set_generic(#generic);
                  ::sepserde::YaSerialize::serialize(item, writer)?;
                }
              })
            }
            /*let (start_event, skip_start) = if field.is_flatten() {
              (quote!(None), true)
            } else {
              (quote!(Some(#label_name.to_string())), false)
            };

            Some(quote! {
              writer.set_start_event_name(#start_event);
              writer.set_skip_start_end(#skip_start);
              ::sepserde::YaSerialize::serialize(&self.#label, writer)?;
            })*/
          }
          Field::Vec { .. } => {
            unimplemented!();
          }
        },
      }
    })
    .collect();

    implement_serializer(
        name,
        root,
        root_attributes,
        append_attributes,
        struct_inspector,
        generics,
    )
}
