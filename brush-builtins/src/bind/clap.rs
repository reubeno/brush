//! `bind` builtin: `BindCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]
use super::BindError;

use clap::{Parser, ValueEnum};
use brush_core::builtins;

/// Inspect and modify key bindings and other input configuration.
#[derive(Parser)]
pub(crate) struct BindCommand {
    /// Name of key map to use.
    #[arg(short = 'm')]
    pub(super) keymap: Option<BindKeyMap>,
    /// List functions.
    #[arg(short = 'l')]
    pub(super) list_funcs: bool,
    /// List functions and bindings.
    #[arg(short = 'P')]
    pub(super) list_funcs_and_bindings: bool,
    /// List functions and bindings in a format suitable for use as input.
    #[arg(short = 'p')]
    pub(super) list_funcs_and_bindings_reusable: bool,
    /// List key sequences that invoke macros.
    #[arg(short = 'S')]
    pub(super) list_key_seqs_that_invoke_macros: bool,
    /// List key sequences that invoke macros in a format suitable for use as input.
    #[arg(short = 's')]
    pub(super) list_key_seqs_that_invoke_macros_reusable: bool,
    /// List variables.
    #[arg(short = 'V')]
    pub(super) list_vars: bool,
    /// List variables in a format suitable for use as input.
    #[arg(short = 'v')]
    pub(super) list_vars_reusable: bool,
    /// Find the keys bound to the given named function.
    #[arg(short = 'q', value_name = "FUNC_NAME")]
    pub(super) query_func_bindings: Option<String>,
    /// Remove all bindings for the given named function.
    #[arg(short = 'u', value_name = "FUNC_NAME")]
    pub(super) remove_func_bindings: Option<String>,
    /// Remove the binding for the given key sequence.
    #[arg(short = 'r', value_name = "KEY_SEQ")]
    pub(super) remove_key_seq_binding: Option<String>,
    /// Import bindings from the given file.
    #[arg(short = 'f', value_name = "PATH")]
    pub(super) bindings_file: Option<String>,
    /// Bind key sequence to command.
    #[arg(short = 'x', value_name = "BINDING")]
    pub(super) key_seq_bindings: Vec<String>,
    /// List key sequence bindings.
    #[arg(short = 'X')]
    pub(super) list_key_seq_bindings: bool,
    /// Key sequence binding to readline function or command.
    pub(super) key_sequence: Option<String>,
}

/// Identifier for a keymap
#[derive(Clone, ValueEnum)]

pub(super) enum BindKeyMap {
    #[clap(name = "emacs-standard", alias = "emacs")]
    EmacsStandard,
    #[clap(name = "emacs-meta")]
    EmacsMeta,
    #[clap(name = "emacs-ctlx")]
    EmacsCtlx,
    #[clap(name = "vi-command", aliases = &["vi", "vi-move"])]
    ViCommand,
    #[clap(name = "vi-insert")]
    ViInsert,
}

impl builtins::Command for BindCommand {
    type Error = BindError;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
