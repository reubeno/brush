use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct AliasCommand {
    #[arg(short = 'p', help = "print all defined aliases in a reusable format")]
    print: bool,

    #[arg(name = "name[=value]")]
    aliases: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for AliasCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        //
        // TODO: implement flags
        // TODO: Don't use println
        //

        let mut exit_code = BuiltinExitCode::Success;

        if self.print || self.aliases.is_empty() {
            for (name, value) in &context.shell.aliases {
                println!("alias {name}='{value}'");
            }
        } else {
            for alias in &self.aliases {
                if let Some((name, unexpanded_value)) = alias.split_once('=') {
                    context
                        .shell
                        .aliases
                        .insert(name.to_owned(), unexpanded_value.to_owned());
                } else if let Some(value) = context.shell.aliases.get(alias) {
                    println!("alias {alias}='{value}'");
                } else {
                    eprintln!("{}: {alias}: not found", context.builtin_name);
                    exit_code = BuiltinExitCode::Custom(1);
                }
            }
        }

        Ok(exit_code)
    }
}
