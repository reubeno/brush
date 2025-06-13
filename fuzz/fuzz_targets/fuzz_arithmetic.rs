#![no_main]
#![allow(missing_docs)]

use std::sync::LazyLock;

use anyhow::Result;
use brush_parser::ast;
use libfuzzer_sys::fuzz_target;

static TOKIO_RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

static SHELL_TEMPLATE: LazyLock<brush_core::Shell> = LazyLock::new(|| {
    let options = brush_core::CreateOptions {
        no_profile: true,
        no_rc: true,
        ..Default::default()
    };
    TOKIO_RT.block_on(brush_core::Shell::new(&options)).unwrap()
});

fn eval_arithmetic(mut shell: brush_core::Shell, input: &ast::ArithmeticExpr) -> Result<()> {
    const DEFAULT_TIMEOUT_IN_SECONDS: u64 = 15;

    //
    // Turn it back into a string so we can pass it in on the command-line.
    //
    let input_str = input.to_string();

    //
    // Instantiate a brush shell with defaults, then try to evaluate the expression.
    //
    let parsed_expr = brush_parser::arithmetic::parse(input_str.as_str()).ok();
    let our_eval_result = if let Some(parsed_expr) = parsed_expr {
        shell.eval_arithmetic(&parsed_expr).ok()
    } else {
        None
    };

    //
    // Now run it under 'bash'
    //
    let mut oracle_cmd = std::process::Command::new("bash");
    oracle_cmd
        .arg("--noprofile")
        .arg("--norc")
        .arg("-O")
        .arg("extglob")
        .arg("-t");

    let mut oracle_cmd = assert_cmd::Command::from_std(oracle_cmd);

    oracle_cmd.timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_IN_SECONDS));

    let input = std::format!("echo \"$(( {input_str} ))\"\n");
    oracle_cmd.write_stdin(input.as_bytes());

    let oracle_result = oracle_cmd.output()?;
    let oracle_eval_result = if oracle_result.status.success() {
        let oracle_output = std::str::from_utf8(&oracle_result.stdout)?;
        oracle_output.trim().parse::<i64>().ok()
    } else {
        None
    };

    //
    // Compare results.
    //
    if our_eval_result != oracle_eval_result {
        Err(anyhow::anyhow!(
            "Mismatched eval results: {oracle_eval_result:?} from oracle vs. {our_eval_result:?} from our test (expr: '{input_str}', oracle result: {oracle_result:?})"
        ))
    } else {
        Ok(())
    }
}

fuzz_target!(|input: ast::ArithmeticExpr| {
    let s = input.to_string();
    let s = s.trim();

    // For now, intentionally ignore known problematic cases without actually running them.
    if s.contains("+ 0")
        || s.is_empty()
        || s.contains(|c: char| c.is_ascii_control() || !c.is_ascii())
        || s.contains("$[")
    // old deprecated form of arithmetic expansion
    {
        return;
    }

    let shell = SHELL_TEMPLATE.clone();
    eval_arithmetic(shell, &input).unwrap();
});
