use std::{borrow::Cow, collections::HashMap};

use clap::Parser;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    variables,
};

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

#[async_trait::async_trait]
impl BuiltinCommand for GetOptsCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut args = HashMap::<char, bool>::new();

        // Build the args map
        let mut last_char = None;
        for c in self.options_string.chars() {
            if c == ':' {
                if let Some(last_char) = last_char {
                    args.insert(last_char, true);
                    continue;
                } else {
                    return Ok(BuiltinExitCode::InvalidUsage);
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
            return Ok(BuiltinExitCode::InvalidUsage);
        }

        let mut next_index_zero_based = next_index - 1;
        if next_index_zero_based >= self.args.len() {
            return Ok(BuiltinExitCode::Custom(1));
        }

        let arg = self.args[next_index_zero_based].as_str();
        if !arg.starts_with('-') || arg.len() != 2 {
            return Ok(BuiltinExitCode::Custom(1));
        }

        // Single character option
        let c = arg.chars().nth(1).unwrap();
        if let Some(takes_arg) = args.get(&c) {
            if *takes_arg {
                next_index += 1;
                next_index_zero_based += 1;

                if next_index_zero_based >= self.args.len() {
                    return Ok(BuiltinExitCode::Custom(1));
                }

                let opt_arg = self.args[next_index_zero_based].as_str();

                context.shell.env.update_or_add(
                    "OPTARG",
                    variables::ShellValueLiteral::Scalar(opt_arg.to_owned()),
                    |_| Ok(()),
                    crate::env::EnvironmentLookup::Anywhere,
                    crate::env::EnvironmentScope::Global,
                )?;
            }

            context.shell.env.update_or_add(
                "OPTIND",
                variables::ShellValueLiteral::Scalar((next_index + 1).to_string()),
                |_| Ok(()),
                crate::env::EnvironmentLookup::Anywhere,
                crate::env::EnvironmentScope::Global,
            )?;

            context.shell.env.update_or_add(
                self.variable_name.as_str(),
                variables::ShellValueLiteral::Scalar(c.to_string()),
                |_| Ok(()),
                crate::env::EnvironmentLookup::Anywhere,
                crate::env::EnvironmentScope::Global,
            )?;
        } else {
            return Ok(BuiltinExitCode::Custom(1));
        }

        Ok(BuiltinExitCode::Success)
    }
}
