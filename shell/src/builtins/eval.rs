use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
pub(crate) struct EvalCommand {
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

impl BuiltinCommand for EvalCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            log::debug!("Applying eval to: {:?}", args_concatenated);

            let exec_result = context
                .shell
                .run_string(args_concatenated.as_str(), false)?;

            Ok(BuiltinExitCode::Custom(exec_result.exit_code))
        } else {
            Ok(BuiltinExitCode::Success)
        }
    }
}
