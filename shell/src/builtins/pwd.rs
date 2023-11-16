use anyhow::Result;
use clap::{arg, Parser};

use crate::context::{BuiltinExitCode, BuiltinResult, ExecutionContext};

#[derive(Parser, Debug)]
struct PwdOptions {
    #[arg(
        short = 'P',
        help = "print the physical directory, without any symbolic links"
    )]
    physical: bool,
    #[arg(
        short = 'L',
        help = "print the value of $PWD if it names the current working directory"
    )]
    allow_symlinks: bool,
}

pub(crate) fn builtin_pwd(context: &mut ExecutionContext, args: &[&str]) -> Result<BuiltinResult> {
    let parse_result = PwdOptions::try_parse_from(args);
    let options = match parse_result {
        Ok(options) => options,
        Err(e) => {
            log::error!("{}", e);
            return Ok(BuiltinResult {
                exit_code: BuiltinExitCode::InvalidUsage,
            });
        }
    };

    //
    // TODO: implement flags
    // TODO: look for 'physical' option in execution context
    //

    if options.physical || options.allow_symlinks {
        log::error!("UNIMPLEMENTED: pwd with -P or -L");
        return Ok(BuiltinResult {
            exit_code: BuiltinExitCode::Unimplemented,
        });
    }

    let cwd = context.working_dir.to_string_lossy().into_owned();

    // TODO: Need to print to whatever the stdout is for the shell.
    println!("{}", cwd);

    Ok(BuiltinResult {
        exit_code: BuiltinExitCode::Success,
    })
}
