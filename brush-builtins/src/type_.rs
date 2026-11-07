use brush_core::writeln_async;
use brush_core::{ExecutionResult, builtins};

use crate::lookup::{self, Resolved};
use tokio::io::AsyncWriteExt as _;

/// Inspect the type of a named shell item.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct TypeCommand {
    /// Display all locations of the specified name, not just the first.
    #[arg(short = 'a')]
    all_locations: bool,

    /// Don't consider functions when resolving the name.
    #[arg(short = 'f')]
    suppress_func_lookup: bool,

    /// Force searching by file path, even if the name is an alias, built-in
    /// command, or shell function.
    #[arg(short = 'P')]
    force_path_search: bool,

    /// Show file path only.
    #[arg(short = 'p')]
    show_path_only: bool,

    /// Only display the type of the specified name.
    #[arg(short = 't')]
    type_only: bool,

    /// Names to search for.
    #[arg(positional)]
    names: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(TypeCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for TypeCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let options = lookup::Options {
            force_path_search: self.force_path_search,
            suppress_func_lookup: self.suppress_func_lookup,
            all_locations: self.all_locations,
            path_dirs: None,
        };

        for name in &self.names {
            let resolved_types = lookup::resolve(context.shell, name, &options);

            if resolved_types.is_empty() {
                if !self.type_only && !self.force_path_search && !self.show_path_only {
                    writeln_async!(context.stderr_async(), "type: {name}: not found").await?;
                }

                result = ExecutionResult::general_error();
                continue;
            }

            for resolved_type in resolved_types {
                if self.show_path_only && !matches!(resolved_type, Resolved::File { .. }) {
                    // Do nothing.
                } else if self.type_only {
                    match &resolved_type {
                        Resolved::Alias(_) => {
                            writeln_async!(context.stdout_async(), "alias").await?;
                        }
                        Resolved::Keyword => {
                            writeln_async!(context.stdout_async(), "keyword").await?;
                        }
                        Resolved::Function(_) => {
                            writeln_async!(context.stdout_async(), "function").await?;
                        }
                        Resolved::Builtin => {
                            writeln_async!(context.stdout_async(), "builtin").await?;
                        }
                        Resolved::File { path, .. } => {
                            if self.show_path_only || self.force_path_search {
                                writeln_async!(
                                    context.stdout_async(),
                                    "{}",
                                    path.to_string_lossy()
                                )
                                .await?;
                            } else {
                                writeln_async!(context.stdout_async(), "file").await?;
                            }
                        }
                    }
                } else {
                    match &resolved_type {
                        // When we're displaying all locations, we don't show hashed paths.
                        Resolved::File { hashed: true, .. }
                            if self.all_locations && !self.force_path_search => {}
                        Resolved::File { path, .. }
                            if self.show_path_only || self.force_path_search =>
                        {
                            writeln_async!(context.stdout_async(), "{}", path.to_string_lossy())
                                .await?;
                        }
                        _ => {
                            let mut text = Vec::new();
                            lookup::describe(&mut text, name, &resolved_type)?;
                            context.stdout_async().write_all(&text).await?;
                        }
                    }
                }

                // If we only want the first, then break after the first.
                if !self.all_locations {
                    break;
                }
            }
        }

        Ok(result)
    }
}
