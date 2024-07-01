#![no_main]

use anyhow::Result;
use libfuzzer_sys::fuzz_target;

async fn run_async(input: String) -> Result<()> {
    //
    // Instantiate a brush shell with defaults, then try to parse the input.
    //
    let options = brush_core::CreateOptions::default();
    let shell = brush_core::Shell::new(&options).await?;
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

    const DEFAULT_TIMEOUT_IN_SECONDS: u64 = 15;
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
        || input.contains("!") // history expansions
        || (input.contains("[") && !input.contains("]")) // ???
        || input.contains("<<") // weirdness with here docs
        || input.ends_with('\\') // unterminated trailing escape char?
        || input.contains("|&")
    // unimplemented bash-ism
    {
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(run_async(input));

    result.unwrap();
});
