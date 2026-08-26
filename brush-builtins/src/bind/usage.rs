//! `bind` builtin: `BindCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use itertools::Itertools as _;
use std::{collections::HashMap, io::Write, str::FromStr as _, sync::Arc};
use strum::IntoEnumIterator;
use tokio::sync::Mutex;
use super::BindError;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Inspect and modify key bindings and other input configuration.
#[derive(usage::Cli)]
#[usage(bin = "bind", unknown_flags = "error", args_override_self = false)]
pub(crate) struct BindCommand {
    /// Name of key map to use.
    #[usage(short = 'm', value_enum)]
    pub(super) keymap: Option<BindKeyMap>,
    /// List functions.
    #[usage(short = 'l')]
    pub(super) list_funcs: bool,
    /// List functions and bindings.
    #[usage(short = 'P')]
    pub(super) list_funcs_and_bindings: bool,
    /// List functions and bindings in a format suitable for use as input.
    #[usage(short = 'p')]
    pub(super) list_funcs_and_bindings_reusable: bool,
    /// List key sequences that invoke macros.
    #[usage(short = 'S')]
    pub(super) list_key_seqs_that_invoke_macros: bool,
    /// List key sequences that invoke macros in a format suitable for use as input.
    #[usage(short = 's')]
    pub(super) list_key_seqs_that_invoke_macros_reusable: bool,
    /// List variables.
    #[usage(short = 'V')]
    pub(super) list_vars: bool,
    /// List variables in a format suitable for use as input.
    #[usage(short = 'v')]
    pub(super) list_vars_reusable: bool,
    /// Find the keys bound to the given named function.
    #[usage(short = 'q', value_name = "FUNC_NAME")]
    pub(super) query_func_bindings: Option<String>,
    /// Remove all bindings for the given named function.
    #[usage(short = 'u', value_name = "FUNC_NAME")]
    pub(super) remove_func_bindings: Option<String>,
    /// Remove the binding for the given key sequence.
    #[usage(short = 'r', value_name = "KEY_SEQ")]
    pub(super) remove_key_seq_binding: Option<String>,
    /// Import bindings from the given file.
    #[usage(short = 'f', value_name = "PATH")]
    pub(super) bindings_file: Option<String>,
    /// Bind key sequence to command.
    #[usage(short = 'x', value_name = "BINDING")]
    pub(super) key_seq_bindings: Vec<String>,
    /// List key sequence bindings.
    #[usage(short = 'X')]
    pub(super) list_key_seq_bindings: bool,
    /// Key sequence binding to readline function or command.
    pub(super) key_sequence: Option<String>,
}

pub(crate) enum BindKeyMap {
    EmacsStandard,
    EmacsMeta,
    EmacsCtlx,
    ViCommand,
    ViInsert,
}

impl std::str::FromStr for BindKeyMap {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "emacs-standard" | "emacs" => Ok(Self::EmacsStandard),
            "emacs-meta" => Ok(Self::EmacsMeta),
            "emacs-ctlx" => Ok(Self::EmacsCtlx),
            "vi-command" | "vi" | "vi-move" => Ok(Self::ViCommand),
            "vi-insert" => Ok(Self::ViInsert),
            _ => Err(format!("invalid keymap: {s}")),
        }
    }
}

crate::impl_usage_parse!(BindCommand);

impl FromArgs for BindCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for BindCommand {
    type Error = BindError;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::usage_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}

/// Errors that can occur while parsing bind arguments.


impl BindKeyMap {
    pub(crate) const fn is_vi(&self) -> bool {
        matches!(self, Self::ViCommand | Self::ViInsert)
    }
}
