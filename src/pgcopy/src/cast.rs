// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use std::io;

pub fn i16<T>(cx: &'static str, v: T) -> Result<i16, io::Error>
where
    i16: TryFrom<T>,
{
    i16::try_from(v).map_err(|_| io::Error::new(io::ErrorKind::Other, concat!(cx, " ", "does not fit into an i16")))
}

pub fn i32<T>(cx: &'static str, v: T) -> Result<i32, io::Error>
where
    i32: TryFrom<T>,
{
    i32::try_from(v).map_err(|_| io::Error::new(io::ErrorKind::Other, concat!(cx, " ", "does not fit into an i32")))
}
