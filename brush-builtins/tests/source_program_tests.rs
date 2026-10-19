//! Tests of sourcing an already parsed program.

#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};

#[tokio::test]
async fn a_parsed_program_is_sourced_as_a_script() -> Result<()> {
    let mut shell = brush_core::Shell::builder()
        .default_builtins(BuiltinSet::BashMode)
        .build()
        .await?;
    let params = shell.default_exec_params();
    let source_info = brush_core::SourceInfo::from(std::path::PathBuf::from("lib.sh"));
    let program = shell.parse_string("seen+=\"$1 \"; return 3; seen+=late")?;

    // Each run gets its own positional parameters, and `return` ends the script, not
    // its caller, with its status.
    for arg in ["a", "b"] {
        let result = shell
            .source_program(&program, &source_info, std::iter::once(arg), &params)
            .await?;
        assert_eq!(u8::from(result.exit_code), 3);
        assert!(!result.is_return_or_exit());
    }
    let result = shell
        .run_string("[[ $seen == 'a b ' ]]", &source_info, &params)
        .await?;
    assert_eq!(u8::from(result.exit_code), 0);
    Ok(())
}
