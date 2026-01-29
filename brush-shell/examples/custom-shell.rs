//! Example: A custom shell with security auditing built on brush.
//!
//! This demonstrates building a production-ready custom shell using brush
//! as a foundation. It shows:
//!
//! - Custom `ShellExtensions` with filters and error formatters
//! - Integrating with `brush-interactive` for readline support
//! - Registering default builtins via `brush_builtins`
//! - Complete shell lifecycle management with interactive mode
//!
//! # Running
//!
//! ```bash
//! cargo run --package brush-shell --example custom-shell
//! ```
//!
//! # Features Demonstrated
//!
//! 1. **Audit Filter**: Logs all executed commands to a shared audit log
//! 2. **Command Blocking**: Blocks dangerous commands like `rm -rf`
//! 3. **Custom Error Formatting**: Colored error output with custom prefix
//! 4. **Interactive Mode**: Full readline support with history and completion

use std::sync::{Arc, Mutex};

use brush_builtins::ShellBuilderExt;
use brush_core::Shell;
use brush_core::extensions::{ErrorFormatter, ShellExtensions};
use brush_core::filter::{
    CmdExecFilter, NoOpSourceFilter, PostFilterResult, PreFilterResult, SimpleCmdOutput,
    SimpleCmdParams,
};

// ============================================================================
// Custom Audit Filter
// ============================================================================

/// Shared state for the auditing filter.
#[derive(Clone, Default)]
struct AuditLog {
    /// Commands that have been executed.
    commands: Arc<Mutex<Vec<String>>>,
}

impl AuditLog {
    /// Records a command to the audit log.
    fn record(&self, command: &str) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(command.to_string());
        }
    }

    /// Returns the number of recorded commands.
    fn count(&self) -> usize {
        self.commands.lock().map(|g| g.len()).unwrap_or_default()
    }
}

/// A command execution filter that audits all commands.
///
/// This filter:
/// - Logs all commands to a shared audit log
/// - Blocks dangerous command patterns (e.g., `rm -rf /`)
/// - Prints audit markers before/after command execution
#[derive(Clone, Default)]
struct AuditingFilter {
    log: AuditLog,
}

impl AuditingFilter {
    /// Creates a new auditing filter.
    fn new() -> Self {
        Self::default()
    }

    /// Returns the audit log for inspection.
    const fn log(&self) -> &AuditLog {
        &self.log
    }

    /// Checks if a command should be blocked.
    fn is_dangerous(command_name: &str, args: &[String]) -> bool {
        // Block `rm -rf /` or `rm -rf /*`
        if command_name == "rm" {
            let has_rf = args.iter().any(|a| a.contains('r') && a.contains('f'));
            let targets_root = args.iter().any(|a| a == "/" || a.starts_with("/*"));
            if has_rf && targets_root {
                return true;
            }
        }
        false
    }
}

impl CmdExecFilter for AuditingFilter {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        let cmd_name = params.command_name().to_string();
        let args: Vec<String> = params.args_to_strings().into_owned();

        // Format the full command for logging
        let full_cmd = if args.len() > 1 {
            format!("{} {}", cmd_name, args[1..].join(" "))
        } else {
            cmd_name.clone()
        };

        // Check for dangerous patterns
        if Self::is_dangerous(&cmd_name, &args) {
            eprintln!("[AUDIT] ❌ BLOCKED dangerous command: {full_cmd}");
            return PreFilterResult::Return(Err(brush_core::error::Error::from(
                brush_core::error::ErrorKind::CommandNotFound(format!(
                    "{cmd_name} (blocked by security policy)"
                )),
            )));
        }

        // Log the command
        self.log.record(&full_cmd);
        eprintln!("[AUDIT] → {full_cmd}");

        PreFilterResult::Continue(params)
    }

    async fn post_simple_cmd(&self, result: SimpleCmdOutput) -> PostFilterResult<SimpleCmdOutput> {
        match &result {
            Ok(_) => eprintln!("[AUDIT] ✓ completed"),
            Err(e) => eprintln!("[AUDIT] ✗ failed: {e}"),
        }
        PostFilterResult::Return(result)
    }
}

// ============================================================================
// Custom Error Formatter
// ============================================================================

/// A custom error formatter with colored output.
#[derive(Clone, Default)]
struct ColoredErrorFormatter;

impl ErrorFormatter for ColoredErrorFormatter {
    fn format_error(
        &self,
        err: &brush_core::error::Error,
        _shell: &Shell<impl ShellExtensions>,
    ) -> String {
        // Use ANSI red for the "error:" prefix
        format!("\x1b[1;31merror:\x1b[0m {err:#}\n")
    }
}

// ============================================================================
// Custom Shell Extensions
// ============================================================================

/// Custom shell extensions combining our audit filter and error formatter.
#[derive(Clone, Default)]
#[allow(dead_code)] // cmd_filter field used indirectly via ShellExtensions
struct CustomExtensions {
    /// The command execution filter.
    cmd_filter: AuditingFilter,
}

impl ShellExtensions for CustomExtensions {
    type ErrorFormatter = ColoredErrorFormatter;
    type CmdExecFilter = AuditingFilter;
    type SourceFilter = NoOpSourceFilter;
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Print banner
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║      Custom Shell Example - brush with Audit Filtering     ║");
    eprintln!("╚════════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("Features enabled:");
    eprintln!("  • Command auditing (all commands logged)");
    eprintln!("  • Security filtering (dangerous patterns blocked)");
    eprintln!("  • Colored error output");
    eprintln!("  • Full interactive mode with readline");
    eprintln!();
    eprintln!("Try: echo hello, ls, rm -rf / (blocked!)");
    eprintln!();

    // Create the audit filter
    let audit_filter = AuditingFilter::new();
    let audit_log = audit_filter.log().clone();

    // Build the shell with custom extensions
    let shell: Shell<CustomExtensions> = Shell::builder_with_extensions()
        .interactive(true)
        .cmd_exec_filter(audit_filter)
        .error_formatter(ColoredErrorFormatter)
        // Add standard builtins
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .build()
        .await?;

    // Wrap the shell for interactive use
    // ShellRef is Arc<Mutex<Shell<SE>>>
    let shell_ref: brush_interactive::ShellRef<CustomExtensions> =
        std::sync::Arc::new(tokio::sync::Mutex::new(shell));

    // Create input backend (reedline for full readline support)
    let ui_options = brush_interactive::UIOptions::default();

    #[cfg(feature = "reedline")]
    let mut input_backend = brush_interactive::ReedlineInputBackend::new(&ui_options, &shell_ref)?;

    #[cfg(not(feature = "reedline"))]
    let mut input_backend = brush_interactive::BasicInputBackend::new(&ui_options, &shell_ref)?;

    // Create and run the interactive shell
    let options = brush_interactive::InteractiveOptions::default();
    let mut interactive =
        brush_interactive::InteractiveShell::new(&shell_ref, &mut input_backend, &options)?;

    interactive.run_interactively().await?;

    // Print audit summary on exit
    eprintln!();
    eprintln!("═══════════════════════════════════════════════════════════");
    eprintln!(
        "Session ended. {} commands were audited.",
        audit_log.count()
    );
    eprintln!("═══════════════════════════════════════════════════════════");

    Ok(())
}
