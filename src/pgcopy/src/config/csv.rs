// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

//! Configuration for the CSV format for `COPY` operations.

/// The default value for [`CopyCsvCommonConfig::delimiter`].
pub const DEFAULT_COPY_CSV_DELIMITER: u8 = b',';

/// The default value for [`CopyCsvCommonConfig::null`].
pub const DEFAULT_COPY_CSV_NULL: &str = "";

/// The default value for [`CopyCsvCommonConfig::quote`].
pub const DEFAULT_COPY_CSV_QUOTE: u8 = b'"';

/// The default value for [`CopyCsvCommonConfig::header`].
pub const DEFAULT_COPY_CSV_HEADER: bool = false;

/// The default value for [`CopyCsvFromConfig::force_not_null`].
pub const DEFAULT_COPY_CSV_FORCE_NOT_NULL: bool = false;

/// The default value for [`CopyCsvFromConfig::force_null`].
pub const DEFAULT_COPY_CSV_FORCE_NULL: bool = false;

/// The default value for [`CopyCsvToConfig::force_quote`].
pub const DEFAULT_COPY_CSV_FORCE_QUOTE: bool = false;

/// Configuration for the CSV format common to both `COPY FROM` and `COPY TO`
/// operations.
#[derive(Debug)]
pub struct CopyCsvCommonConfig {
    /// The character that separates columns within each row.
    ///
    /// Defaults to [`DEFAULT_COPY_CSV_DELIMITER`].
    pub delimiter: u8,
    /// The string that represents the null value.
    ///
    /// Defaults to [`DEFAULT_COPY_TEXT_FORMAT_NULL`].
    pub null: String,
    /// Whether the file contains a header row with the names of each column
    /// in the file.
    ///
    /// Defaults to [`DEFAULT_COPY_CSV_HEADER`].
    pub header: bool,
    /// The quoting character to use for columns that are quoted.
    ///
    /// Defaults to [`DEFAULT_COPY_CSV_QUOTE`].
    pub quote: u8,
    /// The character used to escape the quote character inside of quoted
    /// values.
    ///
    /// Defaults to the configured quote character.
    pub escape: u8,
}

/// Configuration for the CSV format for `COPY FROM` operations.
#[derive(Debug)]
pub struct CopyCsvFromConfig {
    /// Configuration common to `COPY FROM` and `COPY TO` operations.
    pub common: CopyCsvCommonConfig,
    /// For each column in the file, whether to ignore null sentinels.
    ///
    /// If unspecified for a column, defaults to
    /// [`DEFAULT_COPY_CSV_FORCE_NOT_NULL`].
    pub force_not_null: Vec<bool>,
    /// For each column in the file, whether to detect null sentinels even if
    /// the value is quoted.
    ///
    /// If unspecified for a column, defaults to
    /// [`DEFAULT_COPY_CSV_FORCE_NULL`].
    pub force_null: Vec<bool>,
}

/// Configuration for the CSV format for `COPY TO` operations.
#[derive(Debug)]
pub struct CopyCsvToConfig {
    /// Configuration common to `COPY FROM` and `COPY TO` operations.
    pub common: CopyCsvCommonConfig,
    /// For each column in the file, whether to force quoting.
    ///
    /// If unspecified for a column, defaults to
    /// [`DEFAULT_COPY_CSV_FORCE_QUOTE`].
    pub force_quote: Vec<bool>,
}
