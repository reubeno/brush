use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use anyhow::Result;
use clap::Parser;
use std::io::Write;

#[derive(Parser)]
pub(crate) struct UmaskCommand {
    #[arg(
        short = 'p',
        help = "if MODE is omitted, output in a form that may be reused as input"
    )]
    print_roundtrippable: bool,

    #[arg(
        short = 'S',
        help = "makes the output symbolic; otherwise an octal number is given"
    )]
    symbolic_output: bool,

    #[arg(help = "mode mask")]
    mode: Option<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for UmaskCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if let Some(mode) = &self.mode {
            if mode.starts_with('0') {
                let parsed = u32::from_str_radix(mode.as_str(), 8)?;
                context.shell.umask = parsed;
            } else {
                return crate::error::unimp("umask setting mode from symbolic value");
            }
        } else {
            let umask = if self.symbolic_output {
                // TODO: handle symbolic output
                return crate::error::unimp("displaying symbolic output");
            } else {
                let umask_value = context.shell.umask;
                std::format!("0{umask_value:o}")
            };

            if self.print_roundtrippable {
                writeln!(context.stdout(), "umask {umask}")?;
            } else {
                writeln!(context.stdout(), "{umask}")?;
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
