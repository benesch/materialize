// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use crate::config::csv::{CopyCsvFromConfig, CopyCsvToConfig};
use crate::config::text::CopyTextConfig;

pub mod text;
pub mod csv;

/// Configuration for a `COPY FROM` operation.
pub enum CopyFromConfig {
    /// Text format.
    Text(CopyTextConfig),
    /// CSV format.
    Csv(CopyCsvFromConfig),
    /// Binary format.
    Binary,
}

/// Configuration for a `COPY TO` operation.
pub enum CopyToConfig {
    /// Text format.
    Text(CopyTextConfig),
    /// CSV format.
    Csv(CopyCsvToConfig),
    /// Binary format.
    Binary,
}
