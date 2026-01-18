//! Example demonstrating how to use the experimental shell extensions API.
//!
//! This example shows how to create custom filters that intercept shell operations.

use std::sync::Arc;

use anyhow::Result;

use brush_core::commands::SimpleCommand;
use brush_core::filter::{FilterableOp, PostFilterResult, PreFilterResult};
use brush_core::{Shell, SourceInfo, extensions};
use std::sync::Mutex;

// Define a custom extension that logs command executions
struct LoggingExtensions {
    command_count: Arc<Mutex<usize>>,
}

impl LoggingExtensions {
    fn new() -> Self {
        Self {
            command_count: Arc::new(Mutex::new(0)),
        }
    }
}

impl extensions::ShellExtensions for LoggingExtensions {
    fn pre_exec_simple_command<'a>(
        &self,
        input: SimpleCommand<'a>,
    ) -> PreFilterResult<SimpleCommand<'a>> {
        // Increment and log command count
        let count_val = {
            let Ok(mut count) = self.command_count.lock() else {
                // Mutex poisoned, just continue without logging
                return PreFilterResult::Continue(input);
            };
            *count += 1;
            *count
        };

        color_print::ceprintln!(
            "<blue><dim>[FILTER]</dim> Command #{count_val}: {}</blue>",
            input.command_name
        );
        PreFilterResult::Continue(input)
    }

    fn post_exec_simple_command<'a>(
        &self,
        output: <SimpleCommand<'a> as FilterableOp>::Output,
    ) -> PostFilterResult<SimpleCommand<'a>> {
        color_print::ceprintln!(
            "<blue><dim>[FILTER]</dim> Command completed with result: {:?}</blue>",
            output
                .as_ref()
                .map(|_| "success")
                .map_err(|e| e.to_string())
        );

        PostFilterResult::Return(output)
    }

    fn clone_for_subshell(&self) -> Box<dyn extensions::ShellExtensions> {
        // Clone shares the same command counter via Arc
        Box::new(Self {
            command_count: std::sync::Arc::clone(&self.command_count),
        })
    }
}

fn main() -> Result<()> {
    // Build a shell with custom extensions
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async())
}

async fn run_async() -> Result<()> {
    let mut shell = Shell::builder()
        .extensions(Box::new(LoggingExtensions::new()))
        .build()
        .await?;

    // Run simple commands - the filters will log them
    color_print::ceprintln!("<magenta>=== Running first command ===</magenta>");
    let _ = shell
        .run_string(
            "echo 'Hello from filtered shell!'".to_owned(),
            &SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;

    color_print::ceprintln!("\n<magenta>=== Running second command ===</magenta>");
    let _ = shell
        .run_string(
            "pwd".to_owned(),
            &SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;

    color_print::ceprintln!("\n<magenta>=== All commands completed successfully ===</magenta>");
    Ok(())
}
