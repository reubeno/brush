//! Defines a cfg for each input backend this build of the shell offers on the
//! target it's built for: `reedline_backend`, `vt_backend`, `basic_backend`, and
//! `minimal_backend`. A backend's feature can be enabled for any target; it's
//! offered only where it works.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let families = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let unix = families.split(',').any(|family| family == "unix");
    let windows = families.split(',').any(|family| family == "windows");
    let feature = |name: &str| std::env::var_os(format!("CARGO_FEATURE_{name}")).is_some();

    let backends = [
        // reedline draws through crossterm, which builds only for Unix and Windows
        // (as brush-interactive's own build script also decides).
        ("reedline_backend", feature("REEDLINE") && (unix || windows)),
        // The shell's VT and basic backends edit on the process's own terminal
        // (`StdTerminal`), which can switch it to raw mode only on Unix: the
        // Windows console sends keys as bytes only in a mode brush doesn't set,
        // and WASI has no API for raw mode. (brush-interactive's `VtInputBackend`
        // itself builds for every target, for hosts that supply a `Terminal`.)
        ("vt_backend", feature("VT") && unix),
        ("basic_backend", feature("BASIC") && unix),
        ("minimal_backend", feature("MINIMAL")),
    ];
    for (cfg, offered) in backends {
        println!("cargo::rustc-check-cfg=cfg({cfg})");
        if offered {
            println!("cargo::rustc-cfg={cfg}");
        }
    }
}
