//! `getopts` builtin: `GetOptsCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Parse command options.
pub(crate) struct GetOptsCommand {
    /// Specification for options
    pub(super) options_string: String,

    /// Name of variable to receive next option
    pub(super) variable_name: String,

    /// Arguments to parse
    pub(super) args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for GetOptsCommand {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        // N.B. All operands arrive via the trailing-args flow; they are
        // unpacked in [`crate::args::bpaf_support::BpafArgs::set_trailing_args`] below.
        let options_string = bpaf::pure(String::new());
        let variable_name = bpaf::pure(String::new());
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(GetOptsCommand {
            options_string,
            variable_name,
            args,
        })
    }

    fn about() -> &'static str {
        "Parse command options."
    }

    fn synopsis() -> &'static str {
        "OPTSTRING NAME [ARGS]..."
    }

    fn takes_trailing_args() -> bool {
        true
    }

    fn set_trailing_args(&mut self, args: Vec<String>) {
        let mut iter = args.into_iter();
        if let Some(options_string) = iter.next() {
            self.options_string = options_string;
        }
        if let Some(variable_name) = iter.next() {
            self.variable_name = variable_name;
        }
        self.args = iter.collect();
    }
}

impl FromArgs for GetOptsCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for GetOptsCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
