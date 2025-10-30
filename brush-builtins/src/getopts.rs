use std::{borrow::Cow, collections::HashMap, io::Write};

use clap::Parser;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};

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

const VAR_GETOPTS_NEXT_CHAR_INDEX: &str = "__GETOPTS_NEXT_CHAR";

impl builtins::Command for GetOptsCommand {
    type Error = brush_core::Error;

    /// Override the default [`builtins::Command::new`] function to handle clap's limitation related
    /// to `--`. See [`builtins::parse_known`] for more information
    /// TODO: we can safely remove this after the issue is resolved
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let (mut this, rest_args) = brush_core::builtins::try_parse_known::<Self>(args)?;
        if let Some(args) = rest_args {
            this.args.extend(args);
        }
        Ok(this)
    }

    #[expect(clippy::too_many_lines)]
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
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
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
            }

            args.insert(c, false);
            last_char = Some(c);
        }

        // If unset, assume OPTIND is 1.
        let mut next_index: usize = context
            .shell
            .env_str("OPTIND")
            .unwrap_or(Cow::Borrowed("1"))
            .parse()?;

        if next_index < 1 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        let mut new_optarg = None;
        let new_optind;
        let mut variable_value;
        let exit_code;

        // See if there are any args left to parse.
        let mut next_index_zero_based = next_index - 1;
        if next_index_zero_based < self.args.len() {
            let arg = self.args[next_index_zero_based].as_str();

            // See if this is an option.
            if arg.starts_with('-') && arg != "--" {
                // Figure out how far into this option we are.
                const DEFAULT_NEXT_CHAR_INDEX: usize = 1;
                let next_char_index = context
                    .shell
                    .env_str(VAR_GETOPTS_NEXT_CHAR_INDEX)
                    .map_or(DEFAULT_NEXT_CHAR_INDEX, |s| {
                        s.parse().unwrap_or(DEFAULT_NEXT_CHAR_INDEX)
                    });

                // Find the char.
                let c = arg.chars().nth(next_char_index).unwrap();
                let is_last_char_in_option = next_char_index == arg.len() - 1;

                // Look up the char.
                let mut is_error = false;
                if let Some(takes_arg) = args.get(&c) {
                    variable_value = String::from(c);

                    if *takes_arg {
                        // If the option takes a value but it's not the last option in this
                        // argument, then this is an error.
                        if is_last_char_in_option {
                            next_index += 1;
                            next_index_zero_based += 1;

                            if next_index_zero_based >= self.args.len() {
                                return Ok(ExecutionResult::general_error());
                            }

                            new_optarg = Some(self.args[next_index_zero_based].clone());
                        } else {
                            is_error = true;
                        }
                    } else {
                        new_optarg = None;
                    }
                } else {
                    // Set to satisfy compiler.
                    variable_value = String::from("?");

                    is_error = true;
                }

                if is_error {
                    // Unknown option; set variable to '?' and OPTARG to the unknown option (sans
                    // hyphen).
                    variable_value = String::from("?");
                    if !treat_unknown_options_as_failure {
                        new_optarg = Some(String::from(c));
                    } else {
                        new_optarg = None;
                    }

                    // TODO: honor OPTERR=0 indicating suppression of error messages
                    if treat_unknown_options_as_failure {
                        writeln!(context.stderr(), "getopts: illegal option -- {c}")?;
                    }
                }

                if is_last_char_in_option {
                    // We're done with this argument, so unset the internal char index variable
                    // and request an update to OPTIND.
                    new_optind = next_index + 1;
                    context.shell.env.unset(VAR_GETOPTS_NEXT_CHAR_INDEX)?;
                } else {
                    // We have more to go in this argument, so update the internal char index
                    // and request that OPTIND not be updated.
                    new_optind = next_index;
                    context.shell.env.update_or_add(
                        VAR_GETOPTS_NEXT_CHAR_INDEX,
                        variables::ShellValueLiteral::Scalar((next_char_index + 1).to_string()),
                        |_| Ok(()),
                        brush_core::env::EnvironmentLookup::Anywhere,
                        brush_core::env::EnvironmentScope::Global,
                    )?;
                }

                exit_code = ExecutionResult::success();
            } else {
                variable_value = String::from("?");
                new_optarg = None;

                // If it was "--" we encountered, then skip past it.
                if arg == "--" {
                    new_optind = next_index + 1;
                } else {
                    new_optind = next_index;
                }

                // Note that we're done parsing options.
                exit_code = ExecutionResult::general_error();
            }
        } else {
            variable_value = String::from("?");
            new_optarg = None;
            new_optind = next_index;
            exit_code = ExecutionResult::general_error();
        }

        // Update variable value.
        context.shell.env.update_or_add(
            self.variable_name.as_str(),
            variables::ShellValueLiteral::Scalar(variable_value),
            |_| Ok(()),
            brush_core::env::EnvironmentLookup::Anywhere,
            brush_core::env::EnvironmentScope::Global,
        )?;

        // Update OPTARG
        if let Some(new_optarg) = new_optarg {
            context.shell.env.update_or_add(
                "OPTARG",
                variables::ShellValueLiteral::Scalar(new_optarg),
                |_| Ok(()),
                brush_core::env::EnvironmentLookup::Anywhere,
                brush_core::env::EnvironmentScope::Global,
            )?;
        } else {
            let _ = context.shell.env.unset("OPTARG")?;
        }

        // Update OPTIND
        context.shell.env.update_or_add(
            "OPTIND",
            variables::ShellValueLiteral::Scalar(new_optind.to_string()),
            |_| Ok(()),
            brush_core::env::EnvironmentLookup::Anywhere,
            brush_core::env::EnvironmentScope::Global,
        )?;

        Ok(exit_code)
    }
}
