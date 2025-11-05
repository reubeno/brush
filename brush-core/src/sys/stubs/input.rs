//! Terminal input utilities

use crate::{error, interfaces};

/// Translates a key code (byte sequence) into a `Key` enum value. Returns an
/// error if the key code is not recognized.
///
/// This is a stub implementation that returns an unimplemented error.
pub fn get_key_from_key_code(_key_code: &[u8]) -> Result<interfaces::Key, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("translating key code").into())
}
