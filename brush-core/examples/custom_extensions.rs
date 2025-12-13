//! Example demonstrating how to use the experimental shell extensions API.
//!
//! This example shows how to create custom filters that intercept shell operations.
//! To run this example:
//! ```bash
//! cargo run --example custom_extensions --features experimental-filters
//! ```

#[cfg(feature = "experimental-filters")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use brush_core::filter::{OpFilter, PreFilterResult};
    use brush_core::{Shell, extensions};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // Define a custom extension that logs command executions
    struct LoggingExtensions;

    impl extensions::ShellExtensions for LoggingExtensions {
        fn exec_simple_command_filter(&self) -> Option<extensions::ExecSimpleCommandFilter> {
            // Create a filter that logs commands before execution
            struct CommandLogger;

            impl<'a> OpFilter<brush_core::commands::SimpleCommand<'a>> for CommandLogger {
                fn pre_op(
                    &mut self,
                    input: brush_core::commands::SimpleCommand<'a>,
                ) -> PreFilterResult<brush_core::commands::SimpleCommand<'a>> {
                    eprintln!("[FILTER] Executing command: {}", input.command_name);
                    PreFilterResult::Continue(input)
                }
            }

            Some(Arc::new(Mutex::new(CommandLogger)))
        }
    }

    // Build a shell with custom extensions
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let mut shell = Shell::builder()
            .extensions(Box::new(LoggingExtensions))
            .build()
            .await?;

        // Run a simple command - the filter will log it
        let _result = shell
            .run_string(
                "echo 'Hello from filtered shell!'".to_owned(),
                &Default::default(),
                &shell.default_exec_params(),
            )
            .await?;

        eprintln!("Command completed successfully");
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}

#[cfg(not(feature = "experimental-filters"))]
fn main() {
    eprintln!("This example requires the 'experimental-filters' feature");
    eprintln!("Run with: cargo run --example custom_extensions --features experimental-filters");
    std::process::exit(1);
}
