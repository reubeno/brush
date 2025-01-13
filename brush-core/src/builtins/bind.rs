use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error};

/// Inspect and modify key bindings and other input configuration.
#[derive(Parser)]
pub(crate) struct BindCommand {
    /// Name of key map to use.
    #[arg(short = 'm')]
    keymap: Option<String>,
    /// List functions.
    #[arg(short = 'l')]
    list_funcs: bool,
    /// List functions and bindings.
    #[arg(short = 'P')]
    list_funcs_and_bindings: bool,
    /// List functions and bindings in a format suitable for use as input.
    #[arg(short = 'p')]
    list_funcs_and_bindings_reusable: bool,
    /// List key sequences that invoke macros.
    #[arg(short = 'S')]
    list_key_seqs_that_invoke_macros: bool,
    /// List key sequences that invoke macros in a format suitable for use as input.
    #[arg(short = 's')]
    list_key_seqs_that_invoke_macros_reusable: bool,
    /// List variables.
    #[arg(short = 'V')]
    list_vars: bool,
    /// List variables in a format suitable for use as input.
    #[arg(short = 'v')]
    list_vars_reusable: bool,
    /// Find the keys bound to the given named function.
    #[arg(short = 'q')]
    query_func_bindings: Option<String>,
    /// Remove all bindings for the given named function.
    #[arg(short = 'u')]
    remove_func_bindings: Option<String>,
    /// Remove the binding for the given key sequence.
    #[arg(short = 'r')]
    remove_key_seq_binding: Option<String>,
    /// Import bindings from the given file.
    #[arg(short = 'f')]
    bindings_file: Option<String>,
    /// Bind key sequence to command.
    #[arg(short = 'x')]
    key_seq_bindings: Vec<String>,
    /// List key sequence bindings.
    #[arg(short = 'X')]
    list_key_seq_bindings: bool,
}

impl builtins::Command for BindCommand {
    async fn execute(
        self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if self.keymap.is_some() {
            return error::unimp("bind -m is not yet implemented");
        }

        if self.list_funcs {
            return error::unimp("bind -l is not yet implemented");
        }

        if self.list_funcs_and_bindings {
            return error::unimp("bind -P is not yet implemented");
        }

        if self.list_funcs_and_bindings_reusable {
            return error::unimp("bind -p is not yet implemented");
        }

        if self.list_key_seqs_that_invoke_macros {
            return error::unimp("bind -S is not yet implemented");
        }

        if self.list_key_seqs_that_invoke_macros_reusable {
            return error::unimp("bind -s is not yet implemented");
        }

        if self.list_vars {
            return error::unimp("bind -V is not yet implemented");
        }

        if self.list_vars_reusable {
            // For now we'll just display a few items and show defaults.
            writeln!(context.stdout(), "set mark-directories on")?;
            writeln!(context.stdout(), "set mark-symlinked-directories off")?;
        }

        if self.query_func_bindings.is_some() {
            return error::unimp("bind -q is not yet implemented");
        }

        if self.remove_func_bindings.is_some() {
            return error::unimp("bind -u is not yet implemented");
        }

        if self.remove_key_seq_binding.is_some() {
            return error::unimp("bind -r is not yet implemented");
        }

        if self.bindings_file.is_some() {
            return error::unimp("bind -f is not yet implemented");
        }

        if !self.key_seq_bindings.is_empty() {
            return error::unimp("bind -x is not yet implemented");
        }

        if self.list_key_seq_bindings {
            return error::unimp("bind -X is not yet implemented");
        }

        Ok(builtins::ExitCode::Success)
    }
}
