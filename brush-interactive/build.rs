//! Defines `cfg(reedline_backend)` where the reedline input backend is built: with
//! the `reedline` feature, on a target it builds for.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(reedline_backend)");

    // reedline draws through crossterm, which builds only for Unix and Windows.
    // Elsewhere (e.g., on wasm), the feature enables nothing, so it's safe to
    // enable everywhere. Keep this in step with the `[target]` table in
    // Cargo.toml that declares reedline.
    let families = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let supported = families
        .split(',')
        .any(|family| family == "unix" || family == "windows");
    if supported && std::env::var_os("CARGO_FEATURE_REEDLINE").is_some() {
        println!("cargo::rustc-cfg=reedline_backend");
    }
}
