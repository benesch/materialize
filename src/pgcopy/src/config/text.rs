// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

//! Configuration for the text format for `COPY` operations.

/// The default value for [`CopyTextConfig::delimiter`].
pub const DEFAULT_COPY_TEXT_DELIMITER: u8 = b'\t';

/// The default value for [`CopyTextConfig::null`].
pub const DEFAULT_COPY_TEXT_NULL: &str = "\\N";

/// Configuration for the text format for `COPY` operations.
#[derive(Debug)]
pub struct CopyTextConfig {
    /// The character that separates columns within each row.
    ///
    /// Defaults to [`DEFAULT_COPY_FORMAT_DELIMITER`].
    pub delimiter: u8,
    /// The string that represents the null value.
    ///
    /// Defaults to [`DEFAULT_COPY_FORMAT_NULL`].
    pub null: String,
}
