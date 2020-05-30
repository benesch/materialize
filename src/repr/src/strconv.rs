// Copyright Materialize, Inc. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

//! Routines for converting datum values to and from their string
//! representation.
//!
//! The functions in this module are tightly related to the variants of
//! [`ScalarType`](crate::ScalarType). Each variant has a pair of functions in
//! this module named `parse_<variant>` and `format_<variant>`. The type returned by
//! `parse` functions, and the type accepted by `format` functions, will be a
//! type that is easily converted into the [`Datum`](crate::Datum) variant for
//! that type.
//!
//! The functions in this module do not directly convert from `Datum`s to
//! `String`s so that their logic can be reused when `Datum`s are not available
//! or desired, as in the pgrepr crate.
//!
//! The string representations used are exactly the same as the PostgreSQL
//! string representations for the corresponding PostgreSQL type. Deviations
//! should be considered a bug.

use chrono::offset::TimeZone;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use failure::{bail, format_err};

use ore::fmt::FormatBuffer;

use crate::adt::datetime::{self, DateTimeField, ParsedDateTime};
use crate::adt::decimal::Decimal;
use crate::adt::interval::Interval;
use crate::adt::jsonb::Jsonb;

/// Whether the formatted representation can be nested in a list without
/// escaping.
#[derive(Debug)]
pub enum Nestable {
    Yes,
    MayNeedEscaping,
}

/// Parses a boolean value from a string.
///
/// The accepted values are "true", "false", "yes", "no", "on", "off", "1", and
/// "0", or any unambiguous prefix of one of those values. Leading or trailing
/// whitespace is permissible.
pub fn parse_bool(s: &str) -> Result<bool, failure::Error> {
    match s.trim().to_lowercase().as_str() {
        "t" | "tr" | "tru" | "true" | "y" | "ye" | "yes" | "on" | "1" => Ok(true),
        "f" | "fa" | "fal" | "fals" | "false" | "n" | "no" | "of" | "off" | "0" => Ok(false),
        _ => bail!("unable to parse bool"),
    }
}

/// Like [`format_bool`], but returns a string with a static lifetime.
///
/// This function should be preferred to `format_bool` when applicable, as it
/// avoids an allocation.
pub fn format_bool_static(b: bool) -> &'static str {
    match b {
        true => "t",
        false => "f",
    }
}

/// Writes a boolean value into a buffer.
///
/// `true` is encoded as the char `'t'` and `false` is encoded as the char
/// `'f'`.
pub fn format_bool<F>(buf: &mut F, b: bool) -> Nestable
where
    F: FormatBuffer,
{
    buf.write_str(format_bool_static(b));
    Nestable::Yes
}

/// Parses a 32-bit integer from a string.
///
/// Valid values are whatever the [`FromStr`] implementation on [`i32`] accepts,
/// plus leading and trailing whitespace.
pub fn parse_int32(s: &str) -> Result<i32, failure::Error> {
    Ok(s.trim().parse()?)
}

/// Writes a 32-bit integer to a buffer.
///
/// The integer is encoded in base 10.
pub fn format_int32<F>(buf: &mut F, i: i32) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", i);
    Nestable::Yes
}

/// Parses a 64-bit integer from a string.
///
/// Valid values are whatever the [`FromStr`] implementation on [`i64`] accepts,
/// plus leading and trailing whitespace.
pub fn parse_int64(s: &str) -> Result<i64, failure::Error> {
    Ok(s.trim().parse()?)
}

/// Writes a 64-bit integer to a buffer.
///
/// The integer is encoded in base 10.
pub fn format_int64<F>(buf: &mut F, i: i64) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", i);
    Nestable::Yes
}

/// Parses a 32-bit floating-point number from a string.
///
/// Valid values are whatever the [`FromStr`](std::str::FromStr) implementation
/// on [`f32`] accepts plus the following special strings for special
/// floating-point values, plus leading and trailing whitespace.
///
/// Input | Output
/// ------|-------
/// inf, infinity, +inf, +infinity | `f32::INFINITY`
/// -inf, -infinity                | `f32::NEG_INFINITY`
/// nan                            | `f32::NAN`
pub fn parse_float32(s: &str) -> Result<f32, failure::Error> {
    Ok(match s.trim().to_lowercase().as_str() {
        "inf" | "infinity" | "+inf" | "+infinity" => f32::INFINITY,
        "-inf" | "-infinity" => f32::NEG_INFINITY,
        "nan" => f32::NAN,
        s => s.parse()?,
    })
}

/// Writes a 32-bit floating-point number to a buffer.
pub fn format_float32<F>(buf: &mut F, f: f32) -> Nestable
where
    F: FormatBuffer,
{
    if f.is_infinite() {
        if f.is_sign_negative() {
            buf.write_str("-Infinity")
        } else {
            buf.write_str("Infinity")
        }
    } else {
        write!(buf, "{}", f)
    }
    Nestable::Yes
}

/// Parses a 64-bit floating-point number from a string.
pub fn parse_float64(s: &str) -> Result<f64, failure::Error> {
    Ok(match s.trim().to_lowercase().as_str() {
        "inf" | "infinity" | "+inf" | "+infinity" => f64::INFINITY,
        "-inf" | "-infinity" => f64::NEG_INFINITY,
        "nan" => f64::NAN,
        s => s.parse()?,
    })
}

/// Writes a 64-bit floating point number to a buffer.
pub fn format_float64<F>(buf: &mut F, f: f64) -> Nestable
where
    F: FormatBuffer,
{
    if f.is_infinite() {
        if f.is_sign_negative() {
            buf.write_str("-Infinity")
        } else {
            buf.write_str("Infinity")
        }
    } else {
        write!(buf, "{}", f)
    }
    Nestable::Yes
}

/// Uses the following grammar to parse `s` into:
///
/// - `NaiveDate`
/// - `NaiveTime`
/// - Timezone string
///
/// `NaiveDate` and `NaiveTime` are appropriate to compute a `NaiveDateTime`,
/// which can be used in conjunction with a timezone string to generate a
/// `DateTime<Utc>`.
///
/// ```text
/// <unquoted timestamp string> ::=
///     <date value> <space> <time value> [ <time zone interval> ]
/// <date value> ::=
///     <years value> <minus sign> <months value> <minus sign> <days value>
/// <time zone interval> ::=
///     <sign> <hours value> <colon> <minutes value>
/// ```
fn parse_timestamp_string(s: &str) -> Result<(NaiveDate, NaiveTime, i64), failure::Error> {
    if s.is_empty() {
        bail!("Timestamp string is empty!")
    }

    // PostgreSQL special date-time inputs
    // https://www.postgresql.org/docs/12/datatype-datetime.html#id-1.5.7.13.18.8
    // We should add support for other values here, e.g. infinity
    // which @quodlibetor is willing to add to the chrono package.
    if s == "epoch" {
        return Ok((
            NaiveDate::from_ymd(1970, 1, 1),
            NaiveTime::from_hms(0, 0, 0),
            0,
        ));
    }

    let (ts_string, tz_string) = datetime::split_timestamp_string(s);

    let pdt = ParsedDateTime::build_parsed_datetime_timestamp(&ts_string)?;
    let d: NaiveDate = pdt.compute_date()?;
    let t: NaiveTime = pdt.compute_time()?;

    let offset = if tz_string.is_empty() {
        0
    } else {
        datetime::parse_timezone_offset_second(tz_string)?
    };

    Ok((d, t, offset))
}

/// Parses a date from a string.
pub fn parse_date(s: &str) -> Result<NaiveDate, failure::Error> {
    match parse_timestamp_string(s) {
        Ok((date, _, _)) => Ok(date),
        Err(e) => bail!("Invalid DATE '{}': {}", s, e),
    }
}

/// Writes a date to a buffer.
///
/// The date is formatted as `YYYY-MM-DD`, where each component will be padded
/// with leading zeros to reach the specified width.
pub fn format_date<F>(buf: &mut F, d: NaiveDate) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", d);
    // NOTE(benesch): this may be overly conservative. Perhaps dates never
    // have special characters.
    Nestable::MayNeedEscaping
}

/// Parses a `NaiveTime` from `s`, using the following grammar.
///
/// ```text
/// <time value> ::=
///     <hours value> <colon> <minutes value> <colon> <seconds integer value>
///     [ <period> [ <seconds fraction> ] ]
/// ```
pub fn parse_time(s: &str) -> Result<NaiveTime, failure::Error> {
    match ParsedDateTime::build_parsed_datetime_time(&s) {
        Ok(pdt) => pdt.compute_time(),
        Err(e) => bail!("Invalid TIME '{}': {}", s, e),
    }
}

/// Writes a time to a buffer.
pub fn format_time<F>(buf: &mut F, t: NaiveTime) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", t.format("%H:%M:%S"));
    format_nanos(buf, t.nanosecond());
    // NOTE(benesch): this may be overly conservative. Perhaps times never
    // have special characters.
    Nestable::MayNeedEscaping
}

/// Parses a time from a string.
pub fn parse_timestamp(s: &str) -> Result<NaiveDateTime, failure::Error> {
    match parse_timestamp_string(s) {
        Ok((date, time, _)) => Ok(date.and_time(time)),
        Err(e) => bail!("Invalid TIMESTAMP '{}': {}", s, e),
    }
}

/// Writes a timestamp to a buffer.
pub fn format_timestamp<F>(buf: &mut F, ts: NaiveDateTime) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", ts.format("%Y-%m-%d %H:%M:%S"));
    format_nanos(buf, ts.timestamp_subsec_nanos());
    // NOTE(benesch): this may be overly conservative. Perhaps timestamps never
    // have special characters.
    Nestable::MayNeedEscaping
}

/// Parses a timezone-aware timestamp from a string.
pub fn parse_timestamptz(s: &str) -> Result<DateTime<Utc>, failure::Error> {
    let (date, time, offset) = match parse_timestamp_string(s) {
        Ok((date, time, tz_string)) => (date, time, tz_string),
        Err(e) => bail!("Invalid TIMESTAMPTZ '{}': {}", s, e),
    };

    let ts = date.and_time(time);

    let dt_fixed_offset = FixedOffset::east(offset as i32)
        .from_local_datetime(&ts)
        .earliest()
        .ok_or_else(|| format_err!("Invalid tz conversion"))?;

    Ok(DateTime::<Utc>::from_utc(dt_fixed_offset.naive_utc(), Utc))
}

/// Writes a timezone-aware timestamp to a buffer.
pub fn format_timestamptz<F>(buf: &mut F, ts: DateTime<Utc>) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", ts.format("%Y-%m-%d %H:%M:%S+00"));
    format_nanos(buf, ts.timestamp_subsec_nanos());
    // NOTE(benesch): this may be overly conservative. Perhaps timestamptzs
    // never have special characters.
    Nestable::MayNeedEscaping
}

/// Parses an interval from a string.
pub fn parse_interval(s: &str) -> Result<Interval, failure::Error> {
    parse_interval_disambiguated(s, DateTimeField::Second)
}

/// Like [`parse_interval`], but takes a date/time field to identify ambiguous
/// elements.
///
/// For more information about this operation, see
/// [`ParsedDateTime::build_parsed_datetime_interval`].
pub fn parse_interval_disambiguated(
    s: &str,
    d: DateTimeField,
) -> Result<Interval, failure::Error> {
    let pdt = match ParsedDateTime::build_parsed_datetime_interval(&s, d) {
        Ok(pdt) => pdt,
        Err(e) => bail!("Invalid INTERVAL '{}': {}", s, e),
    };

    match pdt.compute_interval() {
        Ok(i) => Ok(i),
        Err(e) => bail!("Invalid INTERVAL '{}': {}", s, e),
    }
}

/// Writes an interval to a buffer.
pub fn format_interval<F>(buf: &mut F, iv: Interval) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", iv);
    Nestable::MayNeedEscaping
}

/// Parses a decimal from a string.
pub fn parse_decimal(s: &str) -> Result<Decimal, failure::Error> {
    s.trim().parse()
}

/// Writes a decimal to a buffer.
pub fn format_decimal<F>(buf: &mut F, d: &Decimal) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", d);
    Nestable::Yes
}

/// Writes a string to a buffer.
pub fn format_string<F>(buf: &mut F, s: &str) -> Nestable
where
    F: FormatBuffer,
{
    buf.write_str(s);
    Nestable::MayNeedEscaping
}

/// Parses a byte vector from a string.
pub fn parse_bytes(s: &str) -> Result<Vec<u8>, failure::Error> {
    // If the input starts with "\x", then the remaining bytes are hex encoded
    // [0]. Otherwise the bytes use the traditional "escape" format. [1]
    //
    // [0]: https://www.postgresql.org/docs/current/datatype-binary.html#id-1.5.7.12.9
    // [1]: https://www.postgresql.org/docs/current/datatype-binary.html#id-1.5.7.12.10
    if s.starts_with("\\x") {
        Ok(hex::decode(&s[2..])?)
    } else {
        parse_bytes_traditional(s.as_bytes())
    }
}

fn parse_bytes_traditional(buf: &[u8]) -> Result<Vec<u8>, failure::Error> {
    // Bytes are interpreted literally, save for the special escape sequences
    // "\\", which represents a single backslash, and "\NNN", where each N
    // is an octal digit, which represents the byte whose octal value is NNN.
    let mut out = Vec::new();
    let mut bytes = buf.iter().fuse();
    while let Some(&b) = bytes.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match bytes.next() {
            None => bail!("bytea input ends with escape character"),
            Some(b'\\') => out.push(b'\\'),
            b => match (b, bytes.next(), bytes.next()) {
                (Some(d2 @ b'0'..=b'3'), Some(d1 @ b'0'..=b'7'), Some(d0 @ b'0'..=b'7')) => {
                    out.push(((d2 - b'0') << 6) + ((d1 - b'0') << 3) + (d0 - b'0'));
                }
                _ => bail!("invalid bytea escape sequence"),
            },
        }
    }
    Ok(out)
}

/// Writes a byte vector into a buffer.
///
/// The bytes are encoded as hex strings and preceded by the string "\x".
///
/// # Examples
///
/// ```rust
/// # use repr::strconv;
/// let mut buf = String::new();
/// strconv::format_bytes(&mut buf, b"hello");
/// assert_eq!(buf, "\\x68656c6c6f");
/// ```
pub fn format_bytes<F>(buf: &mut F, bytes: &[u8]) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "\\x{}", hex::encode(bytes));
    Nestable::Yes
}

/// Parses a JSON object from a string.
///
///
pub fn parse_jsonb(s: &str) -> Result<Jsonb, failure::Error> {
    s.trim().parse()
}

/// Writes a JSON object to a buffer in a compressed format.
pub fn format_jsonb<F>(buf: &mut F, jsonb: &Jsonb) -> Nestable
where
    F: FormatBuffer,
{
    write!(buf, "{}", jsonb);
    Nestable::MayNeedEscaping
}

/// Writes a JSON object to a buffer in a pretty format.
pub fn format_jsonb_pretty<F>(buf: &mut F, jsonb: &Jsonb)
where
    F: FormatBuffer,
{
    write!(buf, "{:#}", jsonb)
}

fn format_nanos<F>(buf: &mut F, mut nanos: u32)
where
    F: FormatBuffer,
{
    if nanos > 0 {
        let mut width = 9;
        while nanos % 10 == 0 {
            width -= 1;
            nanos /= 10;
        }
        write!(buf, ".{:0width$}", nanos, width = width);
    }
}

/// Parses a list from a string.
pub fn parse_list<T>(
    s: &str,
    mut make_null: impl FnMut() -> T,
    mut parse_elem: impl FnMut(&str) -> Result<T, failure::Error>,
) -> Result<Vec<T>, failure::Error> {
    let mut elems = vec![];
    let mut chars = s.chars().peekable();
    match chars.next() {
        // start of list
        Some('{') => (),
        Some(other) => {
            bail!("expected '{{', found {}", other);
        }
        None => bail!("unexpected end of input"),
    }
    loop {
        match chars.peek().copied() {
            // end of list
            Some('}') => {
                // consume
                chars.next();
                match chars.next() {
                    Some(other) => bail!("unexpected leftover input {}", other),
                    None => break,
                }
            }
            // whitespace, ignore
            Some(' ') => {
                // consume
                chars.next();
                continue;
            }
            // an escaped elem
            Some('"') => {
                chars.next();
                let mut elem_text = String::new();
                loop {
                    match chars.next() {
                        // end of escaped elem
                        Some('"') => break,
                        // a backslash-escaped character
                        Some('\\') => match chars.next() {
                            Some('\\') => elem_text.push('\\'),
                            Some('"') => elem_text.push('"'),
                            Some(other) => bail!("bad escape \\{}", other),
                            None => bail!("unexpected end of input"),
                        },
                        // a normal character
                        Some(other) => elem_text.push(other),
                        None => bail!("unexpected end of input"),
                    }
                }
                elems.push(parse_elem(&elem_text)?);
            }
            // a nested list
            Some('{') => {
                let mut elem_text = String::new();
                loop {
                    match chars.next() {
                        Some(c) => {
                            elem_text.push(c);
                            if c == '}' {
                                break;
                            }
                        }
                        None => bail!("unexpected end of input"),
                    }
                }
                elems.push(parse_elem(&elem_text)?);
            }
            // an unescaped elem
            Some(_) => {
                let mut elem_text = String::new();
                loop {
                    match chars.peek().copied() {
                        // end of unescaped elem
                        Some('}') | Some(',') | Some(' ') => break,
                        // a normal character
                        Some(other) => {
                            // consume
                            chars.next();
                            elem_text.push(other);
                        }
                        None => bail!("unexpected end of input"),
                    }
                }
                elems.push(if elem_text.trim() == "NULL" {
                    make_null()
                } else {
                    parse_elem(&elem_text)?
                });
            }
            None => bail!("unexpected end of input"),
        }
        // consume whitespace
        while let Some(' ') = chars.peek() {
            chars.next();
        }
        // look for delimiter
        match chars.next() {
            // another elem
            Some(',') => continue,
            // end of list
            Some('}') => break,
            Some(other) => bail!("expected ',' or '}}', found '{}'", other),
            None => bail!("unexpected end of input"),
        }
    }
    Ok(elems)
}

/// Writes a list to a buffer.
pub fn format_list<F, T>(
    buf: &mut F,
    elems: &[T],
    mut format_elem: impl FnMut(ListElementWriter<F>, &T) -> Nestable,
) -> Nestable
where
    F: FormatBuffer,
{
    buf.write_char('{');
    let mut elems = elems.iter().peekable();
    while let Some(elem) = elems.next() {
        let start = buf.len();
        if let Nestable::MayNeedEscaping = format_elem(ListElementWriter(buf), elem) {
            escape_list_elem(buf, start);
        }
        if elems.peek().is_some() {
            buf.write_char(',')
        }
    }
    buf.write_char('}');
    Nestable::Yes
}

/// Escapes a list element in place.
///
/// The list element must start at `start` and extend to the end of the buffer.
/// The buffer will be resized if escaping is necessary to account for the
/// additional escape characters.
fn escape_list_elem<F>(buf: &mut F, start: usize)
where
    F: FormatBuffer,
{
    let elem = &buf.as_ref()[start..];
    if !elem.is_empty()
        && elem != b"NULL"
        && !elem
            .iter()
            .any(|c| matches!(c, b'{' | b'}' | b',' | b' ' | b'"' | b'\\'))
    {
        // Element does not need escaping.
        return;
    }

    // We'll need two extra bytes for the quotes at the start and end of the
    // element, plus an extra byte for each quote and backslash.
    let extras = 2 + elem.iter().filter(|b| matches!(b, b'"' | b'\\')).count();
    let orig_end = buf.len();
    let new_end = buf.len() + extras;

    // Pad the buffer to the new length. These characters will all be
    // overwritten.
    //
    // NOTE(benesch): we never read these characters, so we could instead use
    // uninitialized memory, but that's a level of unsafety I'm currently
    // uncomfortable with. The performance gain is negligible anyway.
    for _ in 0..extras {
        buf.write_char('\0');
    }

    // SAFETY: inserting ASCII characters before other ASCII characters
    // preserves UTF-8 encoding.
    let elem = unsafe { buf.as_bytes_mut() };

    // Walk the string backwards, writing characters at the new end index while
    // reading from the old end index, adding quotes at the beginning and end,
    // and adding a backslash before every backslash or quote.
    let mut wi = new_end - 1;
    elem[wi] = b'"';
    wi -= 1;
    for ri in (start..orig_end).rev() {
        elem[wi] = elem[ri];
        wi -= 1;
        if let b'\\' | b'"' = elem[ri] {
            elem[wi] = b'\\';
            wi -= 1;
        }
    }
    elem[wi] = b'"';

    assert!(wi == start);
}

/// A helper for [`format_list`] that formats a single list element.
#[derive(Debug)]
pub struct ListElementWriter<'a, F>(&'a mut F);

impl<'a, F> ListElementWriter<'a, F>
where
    F: FormatBuffer,
{
    /// Marks this list element as null.
    pub fn write_null(self) -> Nestable {
        self.0.write_str("NULL");
        Nestable::Yes
    }

    /// Returns a [`FormatBuffer`] into which a non-null element can be
    /// written.
    pub fn nonnull_buffer(self) -> &'a mut F {
        self.0
    }
}
