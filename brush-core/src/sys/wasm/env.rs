//! Environment variable retrieval for wasm platforms.

/// Retrieves environment variables from the host process.
///
/// WASI hosts supply the environment, while targets without one (e.g.,
/// `wasm32-unknown-unknown`) yield no variables. A name or value that isn't valid
/// UTF-8 has its invalid bytes replaced with U+FFFD.
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    crate::sys::lossy_host_env_vars()
}
