use anyhow::Result;
use clap::Parser;

use crate::context::{BuiltinExitCode, BuiltinResult, ExecutionContext};

#[derive(Parser, Debug)]
struct AliasOptions {
    #[arg(short = 'p', help = "print all defined aliases in a reusable format")]
    print: bool,

    #[arg(name = "name[=value]")]
    aliases: Vec<String>,
}

pub(crate) fn builtin_alias(
    context: &mut ExecutionContext,
    args: &[&str],
) -> Result<BuiltinResult> {
    let parse_result = AliasOptions::try_parse_from(args);
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
    // TODO: Don't use println
    //

    let mut exit_code = BuiltinExitCode::Success;

    if options.print || options.aliases.len() == 0 {
        for (name, value) in context.aliases.iter() {
            println!("{}='{}'", name, value);
        }
    } else {
        for alias in options.aliases {
            if let Some((name, unexpanded_value)) = alias.split_once('=') {
                context
                    .aliases
                    .insert(name.to_owned(), unexpanded_value.to_owned());
            } else {
                if let Some(value) = context.aliases.get(&alias) {
                    println!("{}='{}'", alias, value);
                } else {
                    eprintln!("{}: {}: not found", args[0], alias);
                    exit_code = BuiltinExitCode::Custom(1);
                }
            }
        }
    }

    Ok(BuiltinResult { exit_code })
}
