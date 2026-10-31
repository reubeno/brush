//! Generates the config file's JSON schema, which needs `brush-shell` linked:
//! `gen OUTPUT_PATH`.
//!
//! This lives here, rather than in `xtask`, so that the build tooling doesn't
//! need to depend on the shell it builds. It's driven by
//! `cargo xtask gen schema config`.

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let output_path = std::env::args_os()
        .nth(1)
        .context("usage: gen OUTPUT_PATH")?;
    let schema = schemars::schema_for!(brush_shell::config::Config);
    let json = serde_json::to_string_pretty(&schema)?;
    std::fs::write(&output_path, format!("{json}\n"))?;
    Ok(())
}
