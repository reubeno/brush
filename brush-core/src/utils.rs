//! Generic utilities.

use crate::error;

/// Parse the given string as a u8 integer in the specified radix.
///
/// # Arguments
///
/// * `s` - The string to parse.
/// * `radix` - The base to use for parsing.
pub fn parse_str_as_u8(s: &str, radix: u32) -> Result<u8, error::Error> {
    u8::from_str_radix(s, radix).map_err(|inner| {
        error::ErrorKind::IntParseError {
            s: s.to_owned(),
            int_type_name: "u8",
            radix,
            inner,
        }
        .into()
    })
}

/// Parse the given string as a u16 integer in the specified radix.
///
/// # Arguments
///
/// * `s` - The string to parse.
/// * `radix` - The base to use for parsing.
pub fn parse_str_as_u16(s: &str, radix: u32) -> Result<u16, error::Error> {
    u16::from_str_radix(s, radix).map_err(|inner| {
        error::ErrorKind::IntParseError {
            s: s.to_owned(),
            int_type_name: "u16",
            radix,
            inner,
        }
        .into()
    })
}

/// Parse the given string as an i32 integer in the specified radix.
///
/// # Arguments
///
/// * `s` - The string to parse.
/// * `radix` - The base to use for parsing.
pub fn parse_str_as_i32(s: &str, radix: u32) -> Result<i32, error::Error> {
    i32::from_str_radix(s, radix).map_err(|inner| {
        error::ErrorKind::IntParseError {
            s: s.to_owned(),
            int_type_name: "i32",
            radix,
            inner,
        }
        .into()
    })
}

/// Parse the given string as a u32 integer in the specified radix.
///
/// # Arguments
///
/// * `s` - The string to parse.
/// * `radix` - The base to use for parsing.
pub fn parse_str_as_u32(s: &str, radix: u32) -> Result<u32, error::Error> {
    u32::from_str_radix(s, radix).map_err(|inner| {
        error::ErrorKind::IntParseError {
            s: s.to_owned(),
            int_type_name: "u32",
            radix,
            inner,
        }
        .into()
    })
}

/// Parse the given string as a usize integer in the specified radix.
///
/// # Arguments
///
/// * `s` - The string to parse.
/// * `radix` - The base to use for parsing.
pub fn parse_str_as_usize(s: &str, radix: u32) -> Result<usize, error::Error> {
    usize::from_str_radix(s, radix).map_err(|inner| {
        error::ErrorKind::IntParseError {
            s: s.to_owned(),
            int_type_name: "usize",
            radix,
            inner,
        }
        .into()
    })
}
