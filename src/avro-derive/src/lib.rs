// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE file at the
// root of this repository, or online at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// BEGIN LINT CONFIG
// DO NOT EDIT. Automatically generated by bin/gen-lints.
// Have complaints about the noise? See the note in misc/python/materialize/cli/gen-lints.py first.
#![allow(unknown_lints)]
#![allow(clippy::style)]
#![allow(clippy::complexity)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::mutable_key_type)]
#![allow(clippy::stable_sort_primitive)]
#![allow(clippy::map_entry)]
#![allow(clippy::box_default)]
#![allow(clippy::drain_collect)]
#![warn(clippy::bool_comparison)]
#![warn(clippy::clone_on_ref_ptr)]
#![warn(clippy::no_effect)]
#![warn(clippy::unnecessary_unwrap)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![warn(clippy::wildcard_dependencies)]
#![warn(clippy::zero_prefixed_literal)]
#![warn(clippy::borrowed_box)]
#![warn(clippy::deref_addrof)]
#![warn(clippy::double_must_use)]
#![warn(clippy::double_parens)]
#![warn(clippy::extra_unused_lifetimes)]
#![warn(clippy::needless_borrow)]
#![warn(clippy::needless_question_mark)]
#![warn(clippy::needless_return)]
#![warn(clippy::redundant_pattern)]
#![warn(clippy::redundant_slicing)]
#![warn(clippy::redundant_static_lifetimes)]
#![warn(clippy::single_component_path_imports)]
#![warn(clippy::unnecessary_cast)]
#![warn(clippy::useless_asref)]
#![warn(clippy::useless_conversion)]
#![warn(clippy::builtin_type_shadow)]
#![warn(clippy::duplicate_underscore_argument)]
#![warn(clippy::double_neg)]
#![warn(clippy::unnecessary_mut_passed)]
#![warn(clippy::wildcard_in_or_patterns)]
#![warn(clippy::crosspointer_transmute)]
#![warn(clippy::excessive_precision)]
#![warn(clippy::overflow_check_conditional)]
#![warn(clippy::as_conversions)]
#![warn(clippy::match_overlapping_arm)]
#![warn(clippy::zero_divided_by_zero)]
#![warn(clippy::must_use_unit)]
#![warn(clippy::suspicious_assignment_formatting)]
#![warn(clippy::suspicious_else_formatting)]
#![warn(clippy::suspicious_unary_op_formatting)]
#![warn(clippy::mut_mutex_lock)]
#![warn(clippy::print_literal)]
#![warn(clippy::same_item_push)]
#![warn(clippy::useless_format)]
#![warn(clippy::write_literal)]
#![warn(clippy::redundant_closure)]
#![warn(clippy::redundant_closure_call)]
#![warn(clippy::unnecessary_lazy_evaluations)]
#![warn(clippy::partialeq_ne_impl)]
#![warn(clippy::redundant_field_names)]
#![warn(clippy::transmutes_expressible_as_ptr_casts)]
#![warn(clippy::unused_async)]
#![warn(clippy::disallowed_methods)]
#![warn(clippy::disallowed_macros)]
#![warn(clippy::disallowed_types)]
#![warn(clippy::from_over_into)]
// END LINT CONFIG

/// Derive decoders for Rust structs from Avro values.
/// Currently, only the simplest possible case is supported:
/// decoding an Avro record into a struct, each of whose fields
/// is named the same as the corresponding Avro record field
/// and which is in turn decodable without external state.
///
/// Example:
///
/// ```ignore
/// fn make_complicated_decoder() -> impl AvroDecode<Out = SomeComplicatedType> {
///     unimplemented!()
/// }
/// #[derive(AvroDecodable)]
/// struct MyType {
///     x: i32,
///     y: u64,
///     #[decoder_factory(make_complicated_decoder)]
///     z: SomeComplicatedType
/// }
/// ```
///
/// This will create an Avro decoder that expects a record with fields "x", "y", and "z"
/// (and possibly others), where "x" and "y" are of Avro type Int or Long and their
/// values fit in an `i32` or `u64` respectively,
/// and where "z" can be decoded by the decoder returned from `make_complicated_decoder`.
///
/// This crate currently works by generating a struct named (following the example above)
/// MyType_DECODER which is used internally by the `AvroDecodable` implementation.
/// It also requires that the `mz-avro` crate be linked under its default name.
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemStruct};

#[proc_macro_derive(AvroDecodable, attributes(decoder_factory, state_type, state_expr))]
pub fn derive_decodeable(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let state_type = input
        .attrs
        .iter()
        .find(|a| &a.path.get_ident().as_ref().unwrap().to_string() == "state_type")
        .map(|a| a.tokens.clone())
        .unwrap_or(quote! {()});
    let name = input.ident;
    let base_fields: Vec<_> = input
        .fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect();
    let fields: Vec<_> = input
        .fields
        .iter()
        .map(|f| {
            // The type of the field,
            // which must itself be AvroDecodable so that we can recursively
            // decode it.
            let ty = &f.ty;
            let id = f.ident.as_ref().unwrap();
            quote! {
                #id: Option<#ty>
            }
        })
        .collect();

    let field_state_exprs: Vec<_> = input
        .fields
        .iter()
        .map(|f| {
            f.attrs
                .iter()
                .find(|a| &a.path.get_ident().as_ref().unwrap().to_string() == "state_expr")
                .map(|a| a.tokens.clone())
                .unwrap_or(quote! {()})
        })
        .collect();

    let decode_blocks: Vec<_> = input
        .fields
        .iter()
        .zip(field_state_exprs.iter())
        .map(|(f, state_expr)| {
            // The type of the field,
            // which must itself be StatefulAvroDecodable so that we can recursively
            // decode it.
            let ty = &f.ty;
            let id = f.ident.as_ref().unwrap();
            let id_str = id.to_string();
            let found_twice = format!("field `{}` found twice", id);
            let make_decoder =
                if let Some(decoder_factory) = f.attrs.iter().find(|a| {
                    &a.path.get_ident().as_ref().unwrap().to_string() == "decoder_factory"
                }) {
                    let toks = &decoder_factory.tokens;
                    quote! {
                        #toks()
                    }
                } else {
                    quote! {
                        <#ty as ::mz_avro::StatefulAvroDecodable>::new_decoder(#state_expr)
                    }
                };
            quote! {
                #id_str => {
                    if self.#id.is_some() {
                        return Err(::mz_avro::error::Error::Decode(::mz_avro::error::DecodeError::Custom(#found_twice.to_string())));
                    }
                    let decoder = #make_decoder;
                    self.#id = Some(field.decode_field(decoder)?);
                }
            }
        })
        .collect();
    let check_blocks: Vec<_> = input
        .fields
        .iter()
        .map(|f| {
            let id = f.ident.as_ref().unwrap();
            let not_found = format!("field `{}` not found", id);
            quote! {
                let #id = if let Some(#id) = self.#id.take() {
                    #id
                } else {
                    return Err(::mz_avro::error::Error::Decode(::mz_avro::error::DecodeError::Custom(#not_found.to_string())));
                };
            }
        })
        .collect();
    let return_fields: Vec<_> = input
        .fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect();
    let decoder_name = format_ident!("{}_DECODER", name);
    let out = quote! {
        #[derive(Debug)]
        #[allow(non_camel_case_types)]
        struct #decoder_name {
            _STATE: #state_type,
            #(#fields),*
        }
        impl ::mz_avro::AvroDecode for #decoder_name {
            type Out = #name;
            fn record<R: ::mz_avro::AvroRead, A: ::mz_avro::AvroRecordAccess<R>>(
                mut self,
                a: &mut A,
            ) -> ::std::result::Result<#name, ::mz_avro::error::Error> {
                while let Some((name, _idx, field)) = a.next_field()? {
                    match name {
                        #(#decode_blocks)*
                        _ => {
                            field.decode_field(::mz_avro::TrivialDecoder)?;
                        }
                    }
                }
                #(#check_blocks)*
                Ok(#name {
                    #(#return_fields),*
                })
            }
            ::mz_avro::define_unexpected! {
                union_branch, array, map, enum_variant, scalar, decimal, bytes, string, json, uuid, fixed
            }
        }
        impl ::mz_avro::StatefulAvroDecodable for #name {
            type Decoder = #decoder_name;
            type State = #state_type;
            fn new_decoder(state: #state_type) -> #decoder_name {
                #decoder_name {
                    _STATE: state,
                    #(#base_fields: None),*
                }
            }
        }
    };
    TokenStream::from(out)
}
