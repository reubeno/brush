use clap::Parser;

use brush_core::{ExecutionControlFlow, builtins};

/// Continue to the next iteration of a control-flow loop.
#[derive(Parser)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    #[clap(default_value_t = 1)]
    which_loop: i8,
}

brush_clap_adapter::impl_parse_args!(ContinueCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ContinueCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        Ok(crate::loops::leave(
            &context,
            "continue",
            self.which_loop,
            |levels| ExecutionControlFlow::ContinueLoop { levels },
        ))
    }
}
