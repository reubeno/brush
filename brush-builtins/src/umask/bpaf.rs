//! `umask` builtin: `UmaskCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;
use brush_core::ErrorKind;

fn set_umask(value: nix::sys::stat::mode_t) -> Result<(), brush_core::Error> {
    // value of mode_t can be platform dependent
    let mode = nix::sys::stat::Mode::from_bits(value).ok_or_else(|| ErrorKind::InvalidUmask)?;
    nix::sys::stat::umask(mode);
    Ok(())
}

fn symbolic_mask_from_bits(bits: u32) -> String {
    let mut result = String::new();

    if (bits & 0b100) != 0 {
        result.push('r');
    }
    if (bits & 0b010) != 0 {
        result.push('w');
    }
    if (bits & 0b001) != 0 {
        result.push('x');
    }

    result
}

/// Manage the process umask.
#[derive(Bpaf)]
pub(crate) struct UmaskCommand {
    /// If MODE is omitted, output in a form that may be reused as input.
    #[bpaf(short('p'))]
    pub(super) print_roundtrippable: bool,

    /// Makes the output symbolic; otherwise an octal number is given.
    #[bpaf(short('S'))]
    pub(super) symbolic_output: bool,

    /// Mode mask.
    #[bpaf(positional("MODE"))]
    pub(super) mode: Option<String>,
}

impl crate::args::bpaf_support::BpafArgs for UmaskCommand {
fn parser() -> impl bpaf::Parser<Self> {
        umask_command()
    }
fn about() -> &'static str {
        "Manage the process umask."
    }
fn synopsis() -> &'static str {
        "[-pS] [MODE]"
    }
}

impl FromArgs for UmaskCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for UmaskCommand {
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
