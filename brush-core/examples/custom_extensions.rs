//! Example demonstrating how to use the experimental shell extensions API.
//!
//! This example shows how to create custom filters that intercept shell operations.
//! To run this example:
//! ```bash
//! cargo run --example custom_extensions --features experimental-filters
//! ```

#[cfg(feature = "experimental-filters")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use brush_core::filter::PreFilterResult;
    use brush_core::{Shell, SourceInfo, extensions};

    // Define a custom extension that logs command executions
    struct LoggingExtensions {
        command_count: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl LoggingExtensions {
        fn new() -> Self {
            Self {
                command_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }
    }

    impl extensions::ShellExtensions for LoggingExtensions {
        fn pre_exec_simple_command<'a>(
            &self,
            input: brush_core::commands::SimpleCommand<'a>,
        ) -> PreFilterResult<brush_core::commands::SimpleCommand<'a>> {
            // Increment and log command count
            let mut count = self.command_count.lock().unwrap();
            *count += 1;
            eprintln!("[FILTER] Command #{}: {}", *count, input.command_name);

            PreFilterResult::Continue(input)
        }

        fn post_exec_simple_command<'a>(
            &self,
            output: <brush_core::commands::SimpleCommand<'a> as brush_core::filter::FilterableOp>::Output,
        ) -> brush_core::filter::PostFilterResult<brush_core::commands::SimpleCommand<'a>> {
            eprintln!(
                "[FILTER] Command completed with result: {:?}",
                output
                    .as_ref()
                    .map(|_| "success")
                    .map_err(|e| e.to_string())
            );
            brush_core::filter::PostFilterResult::Return(output)
        }

        fn clone_for_subshell(&self) -> Box<dyn extensions::ShellExtensions> {
            // Clone shares the same command counter via Arc
            Box::new(Self {
                command_count: std::sync::Arc::clone(&self.command_count),
            })
        }
    }

    // Build a shell with custom extensions
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let mut shell = Shell::builder()
            .extensions(LoggingExtensions::new())
            .build()
            .await?;

        // Run simple commands - the filters will log them
        eprintln!("=== Running first command ===");
        let _result = shell
            .run_string(
                "echo 'Hello from filtered shell!'".to_owned(),
                &SourceInfo::default(),
                &shell.default_exec_params(),
            )
            .await?;

        eprintln!("\n=== Running second command ===");
        let _result = shell
            .run_string(
                "pwd".to_owned(),
                &SourceInfo::default(),
                &shell.default_exec_params(),
            )
            .await?;

        eprintln!("\n=== All commands completed successfully ===");
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
