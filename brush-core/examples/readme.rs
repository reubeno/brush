//! The embedding snippet shown in the repository README. `tests/readme_snippet.rs`
//! checks that the README's copy matches the marked region below, so the README
//! can never show code that doesn't build.

use anyhow::Result;

async fn run() -> Result<()> {
    // readme:start
    let mut shell = brush_core::Shell::builder().build().await?;

    let result = shell
        .run_string(
            r#"greet() { echo "Hello, $1!"; }; greet world"#,
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;

    assert!(result.is_success());
    // readme:end
    Ok(())
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(run())
}
