use std::{borrow::Cow, collections::HashMap, io::Write};

use clap::Parser;

use crate::{builtins, commands, variables};

/// Parse command options.
#[derive(Parser)]
pub(crate) struct GetOptsCommand {
    /// Specification for options
    options_string: String,

    /// Name of variable to receive next option
    variable_name: String,

    /// Arguments to parse
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for GetOptsCommand {
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut args = HashMap::<char, bool>::new();
        let mut treat_unknown_options_as_failure = true;

        // Build the args map
        let mut last_char = None;
        for c in self.options_string.chars() {
            if c == ':' {
                if let Some(last_char) = last_char {
                    args.insert(last_char, true);
                    continue;
                } else if args.is_empty() {
                    // This is the first character of the options string.
                    // Its presence indicates a request for unknown
                    // options *not* to be treated as failures.
                    treat_unknown_options_as_failure = false;
                } else {
                    return Ok(builtins::ExitCode::InvalidUsage);
                }
            }

            args.insert(c, false);
            last_char = Some(c);
        }

        // TODO: OPTIND is supposed to be initialized to 1 in each shell/script.
        let mut next_index: usize = context
            .shell
            .env
            .get_str("OPTIND")
            .unwrap_or(Cow::Borrowed("1"))
            .parse()?;

        if next_index < 1 {
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        let new_optarg;
        let new_optind;
        let variable_value;
        let exit_code;

        // See if there are any args left to parse.
        let mut next_index_zero_based = next_index - 1;
        if next_index_zero_based < self.args.len() {
            let arg = self.args[next_index_zero_based].as_str();

            // See if this is an option.
            if arg.starts_with('-') && arg.len() == 2 && arg != "--" {
                // Single character option
                let c = arg.chars().nth(1).unwrap();

                // Look up the char.
                if let Some(takes_arg) = args.get(&c) {
                    variable_value = String::from(c);

                    if *takes_arg {
                        next_index += 1;
                        next_index_zero_based += 1;

                        if next_index_zero_based >= self.args.len() {
                            return Ok(builtins::ExitCode::Custom(1));
                        }

                        new_optarg = Some(self.args[next_index_zero_based].clone());
                    } else {
                        new_optarg = None;
                    }
                } else {
                    // Unknown option; set variable to '?' and OPTARG to the unknown option (sans
                    // hyphen).
                    variable_value = String::from("?");
                    if !treat_unknown_options_as_failure {
                        new_optarg = Some(String::from(c));
                    } else {
                        new_optarg = None;
                    }

                    if treat_unknown_options_as_failure {
                        writeln!(context.stderr(), "getopts: illegal option -- {c}")?;
                    }
                }

                new_optind = next_index + 1;
                exit_code = builtins::ExitCode::Success;
            } else {
                variable_value = String::from("?");
                new_optarg = None;
                if arg == "--" {
                    new_optind = next_index + 1;
                } else {
                    new_optind = next_index;
                }
                exit_code = builtins::ExitCode::Custom(1);
            }
        } else {
            variable_value = String::from("?");
            new_optarg = None;
            new_optind = next_index;
            exit_code = builtins::ExitCode::Custom(1);
        }

        // Update variable value.
        context.shell.env.update_or_add(
            self.variable_name.as_str(),
            variables::ShellValueLiteral::Scalar(variable_value),
            |_| Ok(()),
            crate::env::EnvironmentLookup::Anywhere,
            crate::env::EnvironmentScope::Global,
        )?;

        // Update OPTARG
        if let Some(new_optarg) = new_optarg {
            context.shell.env.update_or_add(
                "OPTARG",
                variables::ShellValueLiteral::Scalar(new_optarg),
                |_| Ok(()),
                crate::env::EnvironmentLookup::Anywhere,
                crate::env::EnvironmentScope::Global,
            )?;
        } else {
            let _ = context.shell.env.unset("OPTARG")?;
        }

        // Update OPTIND
        context.shell.env.update_or_add(
            "OPTIND",
            variables::ShellValueLiteral::Scalar(new_optind.to_string()),
            |_| Ok(()),
            crate::env::EnvironmentLookup::Anywhere,
            crate::env::EnvironmentScope::Global,
        )?;

        // Initialize OPTERR
        // TODO: honor OPTERR=0 indicating suppression of error messages
        context.shell.env.update_or_add(
            "OPTERR",
            variables::ShellValueLiteral::Scalar("1".to_string()),
            |_| Ok(()),
            crate::env::EnvironmentLookup::Anywhere,
            crate::env::EnvironmentScope::Global,
        )?;

        Ok(exit_code)
    }
}
