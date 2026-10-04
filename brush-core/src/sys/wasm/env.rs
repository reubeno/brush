//! Environment variable retrieval for wasm platforms.

/// Retrieves environment variables from the host process.
///
/// A direct passthrough to [`std::env::vars()`]: WASI hosts supply the
/// environment, while targets without one (e.g., `wasm32-unknown-unknown`)
/// yield no variables.
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    std::env::vars()
}
