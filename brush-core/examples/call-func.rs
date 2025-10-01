#![allow(missing_docs)]

use anyhow::Result;

async fn instantiate_shell() -> Result<brush_core::Shell> {
    let shell = brush_core::Shell::builder().build().await?;
    Ok(shell)
}

async fn define_func(shell: &mut brush_core::Shell) -> Result<()> {
    let script = r#"hello() { echo "Hello, world: $@"; return 42; }
"#;

    let result = shell
        .run_string(script, &shell.default_exec_params())
        .await?;

    eprintln!("[Function definition result: {}]", result.is_success());

    Ok(())
}

async fn run_func(shell: &mut brush_core::Shell, suppress_stdout: bool) -> Result<()> {
    let mut params = shell.default_exec_params();

    if suppress_stdout {
        params
            .open_files
            .set(brush_core::OpenFiles::STDOUT_FD, brush_core::OpenFile::Null);
    }

    let result = shell
        .invoke_function("hello", std::iter::once("arg"), &params)
        .await?;

    eprintln!("[Function invocation result: {result}]");

    Ok(())
}

async fn run(suppress_stdout: bool) -> Result<()> {
    let mut shell = instantiate_shell().await?;

    define_func(&mut shell).await?;

    for (name, _) in shell.funcs.iter() {
        eprintln!("[Found function: {name}]");
    }

    run_func(&mut shell, suppress_stdout).await?;

    Ok(())
}

fn main() -> Result<()> {
    const SUPPRESS_STDOUT: bool = true;

    // Construct a runtime for us to run async code on.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(run(SUPPRESS_STDOUT))?;

    Ok(())
}
