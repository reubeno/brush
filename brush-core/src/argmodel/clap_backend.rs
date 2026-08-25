//! The clap implementation of the argument-model backend.
//!
//! Builds a [`clap::Command`] from the neutral spec and maps
//! `ArgMatches` back onto declaration ids.

#![cfg(feature = "parser-clap")]

use super::model::{ArgKind, CommandSpec, ParsedValues};
use crate::builtins::BuiltinArgParseError;

/// The clap backend.
pub struct ClapBackend;

impl super::ArgParserBackend for ClapBackend {
    fn parse(
        &self,
        spec: &'static CommandSpec,
        name: &str,
        argv: &[String],
    ) -> Result<ParsedValues, BuiltinArgParseError> {
        let mut command = build_command(spec, name);

        // N.B. clap treats argv[0] as the program name; our callers hand us
        // words only, so prepend an empty placeholder.
        let mut clap_argv: Vec<String> = vec![String::new()];
        clap_argv.extend(argv.iter().cloned());
        let matches = match command.try_get_matches_from_mut(clap_argv) {
            Ok(matches) => matches,
            Err(err) => {
                let help_request = matches!(
                    err.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                );
                return Err(BuiltinArgParseError {
                    message: err.to_string(),
                    help_request,
                });
            }
        };

        let mut out = ParsedValues::new(spec);
        for arg in spec.args {
            if arg.kind == ArgKind::Flag {
                if matches.get_flag(arg.id) {
                    out.set_flag(arg.id);
                }
                continue;
            }
            if let Some(values) = matches.get_many::<String>(arg.id) {
                for value in values.cloned().collect::<Vec<_>>() {
                    out.push_value(arg.id, value);
                }
            } else if let Some(value) = matches.get_one::<String>(arg.id) {
                out.push_value(arg.id, value.clone());
            }
        }

        // Trailing verbatim operands are captured by the core splitter before
        // this backend runs; nothing positional reaches clap here unless a
        // workload declared one explicitly.
        for pos in spec.positionals {
            if let Some(values) = matches.get_many::<String>(pos.id) {
                out.set_values(pos.id, values.cloned().collect());
            } else if let Some(value) = matches.get_one::<String>(pos.id) {
                out.push_value(pos.id, value.clone());
            }
        }

        Ok(out)
    }

    fn detailed_help(&self, spec: &CommandSpec, name: &str) -> Result<String, crate::error::Error> {
        Ok(build_command(spec, name).render_help().to_string())
    }
}

fn build_command(spec: &CommandSpec, name: &str) -> clap::Command {
    // N.B. Without clap's `string` feature, names must be 'static; intern by
    // leaking (bounded by distinct builtin invocations).
    let static_name: &'static str = Box::leak(name.to_owned().into_boxed_str());
    let mut command = clap::Command::new(static_name)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .override_usage({
            // Keep clap from printing its own synthesized usage line style;
            // brush renders short usage itself from `synopsis`.
            std::format!("{name} [OPTIONS]")
        });

    for arg in spec.args {
        let mut longs = arg.longs.iter();
        let first_long = longs.next().copied();

        let mut cmd_arg = match arg.shorts.first() {
            Some(short) => clap::Arg::new(arg.id).short(char::from(*short)),
            None => clap::Arg::new(arg.id),
        };
        if let Some(long) = first_long {
            cmd_arg = cmd_arg.long(long);
        }
        for alias in longs {
            cmd_arg = cmd_arg.alias(alias);
        }

        cmd_arg = match arg.kind {
            ArgKind::Flag => cmd_arg.action(clap::ArgAction::SetTrue),
            ArgKind::Value => cmd_arg
                .action(clap::ArgAction::Append)
                .num_args(1)
                .value_name(arg.metavar.unwrap_or("VALUE")),
        };

        if arg.hidden {
            cmd_arg = cmd_arg.hide(true);
        }
        if !arg.help.is_empty() {
            cmd_arg = cmd_arg.help(arg.help);
        }

        command = command.arg(cmd_arg);
    }

    for pos in spec.positionals {
        let mut cmd_arg = clap::Arg::new(pos.id)
            .value_name(pos.name)
            .action(clap::ArgAction::Append);
        cmd_arg = if pos.many {
            cmd_arg
                .num_args(1..)
                .trailing_var_arg(true)
                .allow_hyphen_values(true)
        } else {
            cmd_arg.num_args(1)
        };
        command = command.arg(cmd_arg);
    }

    command
}
