//! Defines a `pty` cfg for platforms that have a working PTY backend (i.e.
//! where the `expectrl` library we use for pty-based tests is supported).
//!
//! Keep this predicate in sync with the `[target.'cfg(...)'.dependencies]`
//! section in `Cargo.toml` (Cargo manifests can't reference custom cfg names).

fn main() {
    cfg_aliases::cfg_aliases! {
        pty: { any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "freebsd") },
    }
}
