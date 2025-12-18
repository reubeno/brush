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
    /// TODO(command): we can safely remove this after the issue is resolved
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
        let mut option_defs = HashMap::<char, bool>::new();
        let mut treat_unknown_options_as_failure = true;

        // Build the map of option definitions from the options spec string.
        let mut last_char = None;
        for c in self.options_string.chars() {
            if c == ':' {
                if let Some(last_char) = last_char {
                    option_defs.insert(last_char, true);
                    continue;
                } else if option_defs.is_empty() {
                    // This is the first character of the options string.
                    // Its presence indicates a request for unknown
                    // options *not* to be treated as failures.
                    treat_unknown_options_as_failure = false;
                } else {
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
            }

            option_defs.insert(c, false);
            last_char = Some(c);
        }

        // If unset, assume OPTIND is 1.
        let next_index_str = context
            .shell
            .env_str("OPTIND")
            .unwrap_or(Cow::Borrowed("1"));
        let mut next_index = brush_core::utils::parse_str_as_usize(next_index_str.as_ref(), 10)?;

        if next_index < 1 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        let mut new_optarg = None;
        let new_optind;
        let mut variable_value;
        let exit_code;

        // Select the arguments to parse. If none were explicitly provided, we
        // default to using the shell's current positional parameters.
        let args_to_parse = if !self.args.is_empty() {
            &self.args
        } else {
            context.shell.current_shell_args()
        };

        // See if there are any args left to parse.
        let mut next_index_zero_based = next_index - 1;
        if next_index_zero_based < args_to_parse.len() {
            let arg = args_to_parse[next_index_zero_based].as_str();

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
                let mut is_last_char_in_option = next_char_index == arg.len() - 1;

                // Look up the char.
                let mut is_error = false;
                if let Some(takes_arg) = option_defs.get(&c) {
                    variable_value = String::from(c);

                    if *takes_arg {
                        // This option takes a value. If it's the last character in the option,
                        // then we need to look for its value in the next argument. If it's
                        // not, then the rest of this argument will be its value.
                        if is_last_char_in_option {
                            next_index += 1;
                            next_index_zero_based += 1;

                            if next_index_zero_based >= args_to_parse.len() {
                                return Ok(ExecutionResult::general_error());
                            }

                            new_optarg = Some(args_to_parse[next_index_zero_based].clone());
                        } else {
                            new_optarg = Some(arg.chars().skip(next_char_index + 1).collect());

                            // Note that we have reached the end of the option, and we'll be ready
                            // to move the next argument.
                            is_last_char_in_option = true;
                        }
                    } else {
                        // This option doesn't take a value.
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

                    // TODO(getopts): honor OPTERR=0 indicating suppression of error messages
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
