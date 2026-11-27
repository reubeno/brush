//! Environment variable retrieval for Unix platforms.

/// Retrieves environment variables from the host process.
///
/// A name or value that isn't valid UTF-8 has its invalid bytes replaced with U+FFFD.
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    crate::sys::lossy_host_env_vars()
}
