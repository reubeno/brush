#![no_main]
#![allow(missing_docs)]

use anyhow::Result;
use libfuzzer_sys::fuzz_target;
use std::sync::LazyLock;

static TOKIO_RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

static SHELL_TEMPLATE: LazyLock<brush_core::Shell> = LazyLock::new(|| {
    brush_core::Shell::builder()
        .no_profile(true)
        .no_rc(true)
        .build()
        .unwrap()
});

#[expect(clippy::unused_async)]
async fn parse_async(shell: brush_core::Shell, input: String) -> Result<()> {
    const DEFAULT_TIMEOUT_IN_SECONDS: u64 = 15;

    //
    // Instantiate a brush shell with defaults, then try to parse the input.
    //
    let our_parse_result = shell.parse_string(input.clone());

    //
    // Now run it under 'bash -n -t' as a crude way to see if it's at least valid syntax.
    //
    let mut oracle_cmd = std::process::Command::new("bash");
    oracle_cmd
        .arg("--noprofile")
        .arg("--norc")
        .arg("-O")
        .arg("extglob")
        .arg("-n")
        .arg("-t");

    let mut oracle_cmd = assert_cmd::Command::from_std(oracle_cmd);

    oracle_cmd.timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_IN_SECONDS));

    let mut input = input;
    input.push('\n');
    oracle_cmd.write_stdin(input.as_bytes());

    let oracle_result = oracle_cmd.output()?;

    //
    // Compare results.
    //
    if our_parse_result.is_ok() != oracle_result.status.success() {
        Err(anyhow::anyhow!(
            "Mismatched parse results: {oracle_result:?} vs {our_parse_result:?}"
        ))
    } else {
        Ok(())
    }
}

fuzz_target!(|input: String| {
    // Ignore known problematic cases without actually running them.
    if input.is_empty()
        || input.contains(|c: char| c.is_ascii_control() || !c.is_ascii()) // non-ascii chars (or control sequences)
        || input.contains('!') // history expansions
        || (input.contains('[') && !input.contains(']')) // ???
        || input.contains("<<") // weirdness with here docs
        || input.ends_with('\\') // unterminated trailing escape char?
        || input.contains("|&")
    // unimplemented bash-ism
    {
        return;
    }

    let shell = SHELL_TEMPLATE.clone();
    TOKIO_RT.block_on(parse_async(shell, input)).unwrap();
});
