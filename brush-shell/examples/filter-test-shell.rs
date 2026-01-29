//! A minimal shell for testing filter functionality.
//!
//! This example creates a simple non-interactive shell with an auditing filter
//! that can be used for integration testing. It accepts commands via `-c` flag
//! just like bash/brush.
//!
//! # Running
//!
//! ```bash
//! cargo build --package brush-shell --example filter-test-shell
//! ./target/debug/examples/filter-test-shell -c "echo hello"
//! ```
//!
//! # Filter Behavior
//!
//! - All commands are logged with `[AUDIT] →` prefix to stderr
//! - Commands named "blocked" are blocked and return exit code 127

use std::sync::{Arc, Mutex};

use brush_builtins::ShellBuilderExt;
use brush_core::Shell;
use brush_core::extensions::{DefaultErrorFormatter, ShellExtensions};
use brush_core::filter::{
    CmdExecFilter, NoOpSourceFilter, PostFilterResult, PreFilterResult, SimpleCmdOutput,
    SimpleCmdParams,
};

// ============================================================================
// Audit Filter
// ============================================================================

/// Shared audit log state.
#[derive(Clone, Default)]
struct AuditLog {
    count: Arc<Mutex<u32>>,
}

impl AuditLog {
    fn increment(&self) {
        if let Ok(mut count) = self.count.lock() {
            *count += 1;
        }
    }
}

/// A simple auditing filter for testing.
#[derive(Clone, Default)]
struct TestAuditFilter {
    log: AuditLog,
}

impl CmdExecFilter for TestAuditFilter {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        let cmd_name = params.command_name();

        // Block commands named "blocked"
        if cmd_name == "blocked" {
            eprintln!("[AUDIT] BLOCKED: {cmd_name}");
            return PreFilterResult::Return(Err(brush_core::error::Error::from(
                brush_core::error::ErrorKind::CommandNotFound("blocked (by filter)".to_string()),
            )));
        }

        // Log the command
        self.log.increment();
        eprintln!("[AUDIT] → {cmd_name}");

        PreFilterResult::Continue(params)
    }

    async fn post_simple_cmd(&self, result: SimpleCmdOutput) -> PostFilterResult<SimpleCmdOutput> {
        match &result {
            Ok(_) => eprintln!("[AUDIT] ✓ ok"),
            Err(e) => eprintln!("[AUDIT] ✗ error: {e}"),
        }
        PostFilterResult::Return(result)
    }
}

// ============================================================================
// Shell Extensions
// ============================================================================

/// Custom extensions for the test shell.
#[derive(Clone, Default)]
struct TestExtensions;

impl ShellExtensions for TestExtensions {
    type ErrorFormatter = DefaultErrorFormatter;
    type CmdExecFilter = TestAuditFilter;
    type SourceFilter = NoOpSourceFilter;
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Parse -c argument
    let command = if args.len() >= 3 && args[1] == "-c" {
        Some(args[2..].join(" "))
    } else if args.len() == 2 && args[1] == "-c" {
        eprintln!("filter-test-shell: -c: option requires an argument");
        std::process::exit(2);
    } else if args.len() > 1 {
        eprintln!("Usage: filter-test-shell -c 'command'");
        std::process::exit(2);
    } else {
        None
    };

    // Create filter
    let filter = TestAuditFilter::default();

    // Build shell with standard builtins
    let mut shell: Shell<TestExtensions> = Shell::builder_with_extensions()
        .cmd_exec_filter(filter)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .build()
        .await?;

    // Run command if provided
    if let Some(cmd) = command {
        let source_info = brush_core::SourceInfo::from("<command line>");
        let params = shell.default_exec_params();
        let result = shell.run_string(&cmd, &source_info, &params).await?;
        std::process::exit(i32::from(u8::from(result.exit_code)));
    }

    Ok(())
}
