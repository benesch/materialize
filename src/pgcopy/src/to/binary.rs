// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use std::mem;
use std::io;

use bytes::BytesMut;
use mz_repr::Row;
use mz_repr::{RelationDesc, ColumnType};

use crate::cast;

const NULL_ENCODING: [u8; 4] = (-1i32).to_be_bytes();

pub struct CopyToBinary<'a> {
    field_count: i16,
    field_types: &'a [ColumnType],
    field_pgtypes: Vec<mz_pgrepr::Type>,
    scratch_buf: BytesMut,
    out_buf: Vec<u8>,
}

impl CopyToBinary<'_> {
    pub fn new(desc: &RelationDesc) -> Result<Self, io::Error> {
        let field_count = cast::i16("field count", desc.arity())?;
        let field_types = &desc.typ().column_types;
        let field_pgtypes = field_types.iter().map(|ty| mz_pgrepr::Type::from(&ty.scalar_type)).collect();

        let scratch_buf = BytesMut::new();
        let mut out_buf = vec![];
        // Write header.
        // 11-byte signature.
        out_buf.extend(b"PGCOPY\n\xFF\r\n\0");
        // 32-bit flags field.
        out_buf.extend([0, 0, 0, 0]);
        // 32-bit header extension length field.
        out_buf.extend([0, 0, 0, 0]);

        Ok(CopyToBinary {
            field_count,
            field_types,
            field_pgtypes,
            scratch_buf,
            out_buf,
        })
    }

    pub fn encode_row(&mut self, row: &Row) {
        // Each row starts with the field count as a 16-bit integer.
        self.out_buf.extend(self.field_count.to_be_bytes());
        for (datum, ty, pgty) in row.iter().zip_eq(self.field_types).zip_eq(self.field_pgtypes) {
            let value = mz_pgrepr::Value::from_datum(datum, &ty.scalar_type);
            match value {
                None => self.out_buf.extend(NULL_ENCODING),
                Some(value) => {
                    self.scratch_buf.clear();
                    value.encode_binary(ty, buf)
                }
            }
        }

    }

    pub fn flush(&mut self) -> Vec<u8> {
        mem::take(&mut self.out_buf)
    }

    pub fn finish(mut self) -> Vec<u8> {
        let trailer: i16 = -1;
        self.out_buf.extend(trailer.to_be_bytes());
        self.out_buf
    }
}
