//! Terminal input utilities

use crate::{error, interfaces};

/// Translates a key code (byte sequence) into a `Key` enum value. Returns `None`
/// if the key code is not recognized.
///
/// This is a stub implementation that always returns `None`.
pub fn try_get_key_from_key_code(_key_code: &[u8]) -> Option<interfaces::Key> {
    None
}
