//! Example demonstrating stateful command filters and filter composition.
//!
//! This example shows how to:
//! - Create custom filters with shared state
//! - Block specific commands from executing
//! - Compose multiple filters using `FilterStack` and `.and_then()`
//! - Use rate limiting to prevent command abuse
//!
//! Run with: `cargo run --package brush-core --example auditing-filter`

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use brush_core::Shell;
use brush_core::extensions::{DefaultErrorFormatter, ShellExtensions};
use brush_core::filter::{
    CmdExecFilter, CmdExecFilterExt, FilterStack, NoOpSourceFilter, PostFilterResult,
    PreFilterResult, SimpleCmdOutput, SimpleCmdParams,
};

// ============================================================================
// Auditing Filter - logs all executed commands
// ============================================================================

/// Shared state for the auditing filter.
#[derive(Clone, Default)]
struct AuditLog {
    /// Commands that have been executed.
    commands: Arc<Mutex<Vec<String>>>,
    /// Commands that are blocked from execution.
    blocked_commands: Arc<HashSet<String>>,
}

impl AuditLog {
    /// Creates a new audit log with the given blocked commands.
    fn with_blocked(blocked: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            commands: Arc::new(Mutex::new(Vec::new())),
            blocked_commands: Arc::new(blocked.into_iter().map(Into::into).collect()),
        }
    }

    /// Returns the logged commands.
    fn logged_commands(&self) -> Vec<String> {
        self.commands.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// A command execution filter that audits all commands.
#[derive(Clone, Default)]
struct AuditingCmdFilter {
    log: AuditLog,
}

impl AuditingCmdFilter {
    /// Creates a new auditing filter with the given blocked commands.
    fn with_blocked(blocked: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            log: AuditLog::with_blocked(blocked),
        }
    }

    /// Returns the audit log.
    const fn log(&self) -> &AuditLog {
        &self.log
    }
}

impl CmdExecFilter for AuditingCmdFilter {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        let cmd_name = params.command_name().to_string();
        // Use args_to_strings() for lazy allocation - only allocates if filter inspects args
        let args = params.args_to_strings();
        // args[0] is the command name, so skip it for display
        let args_display = if args.len() > 1 {
            args[1..].join(" ")
        } else {
            String::new()
        };

        // Check if command is blocked
        if self.log.blocked_commands.contains(&cmd_name) {
            println!("  [AUDIT] ❌ BLOCKED: {cmd_name} {args_display}");
            // Return an error indicating the command was blocked
            return PreFilterResult::Return(Err(brush_core::error::Error::from(
                brush_core::error::ErrorKind::CommandNotFound(format!(
                    "{cmd_name} (blocked by policy)"
                )),
            )));
        }

        // Log the command
        let full_cmd = if args_display.is_empty() {
            cmd_name
        } else {
            format!("{cmd_name} {args_display}")
        };
        println!("  [AUDIT] → Executing: {full_cmd}");
        if let Ok(mut commands) = self.log.commands.lock() {
            commands.push(full_cmd);
        }

        PreFilterResult::Continue(params)
    }

    async fn post_simple_cmd(&self, result: SimpleCmdOutput) -> PostFilterResult<SimpleCmdOutput> {
        match &result {
            Ok(_) => println!("  [AUDIT] ✓ Command completed"),
            Err(e) => println!("  [AUDIT] ✗ Command failed: {e}"),
        }
        PostFilterResult::Return(result)
    }
}

// ============================================================================
// Rate Limit Filter - limits how many times each command can be run
// ============================================================================

/// A command execution filter that rate-limits command invocations.
///
/// This demonstrates a second independent filter that can be composed
/// with the auditing filter.
#[derive(Clone)]
struct RateLimitFilter {
    /// Per-command invocation counts.
    counts: Arc<Mutex<HashMap<String, u32>>>,
    /// Maximum invocations allowed per command.
    max_per_command: u32,
}

impl Default for RateLimitFilter {
    fn default() -> Self {
        Self::new(10)
    }
}

impl RateLimitFilter {
    /// Creates a new rate limit filter with the given maximum invocations per command.
    fn new(max_per_command: u32) -> Self {
        Self {
            counts: Arc::new(Mutex::new(HashMap::new())),
            max_per_command,
        }
    }
}

impl CmdExecFilter for RateLimitFilter {
    #[allow(clippy::significant_drop_tightening)]
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        let cmd_name = params.command_name().to_string();

        let count = {
            let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
            let count = counts.entry(cmd_name.clone()).or_insert(0);
            *count += 1;
            *count
            // Lock dropped here at end of block
        };

        if count > self.max_per_command {
            println!(
                "  [RATE]  🚫 Rate limit exceeded for '{cmd_name}' ({count}/{})",
                self.max_per_command
            );
            return PreFilterResult::Return(Err(brush_core::error::Error::from(
                brush_core::error::ErrorKind::CommandNotFound(format!(
                    "{cmd_name} (rate limit exceeded)"
                )),
            )));
        }

        println!(
            "  [RATE]  📊 '{cmd_name}' invocation {count}/{}",
            self.max_per_command
        );
        PreFilterResult::Continue(params)
    }
}

// ============================================================================
// Composed Extensions - combining multiple filters
// ============================================================================

/// The composed filter type: `AuditingCmdFilter` runs first, then `RateLimitFilter`.
///
/// This demonstrates the `FilterStack` type that results from composition.
/// You can also use `.and_then()` for the same effect (shown in main).
type ComposedFilter = FilterStack<AuditingCmdFilter, RateLimitFilter>;

/// Custom shell extensions with composed filters.
///
/// This shows how to wire a composed filter into the shell's type system.
#[derive(Clone, Default)]
#[allow(dead_code)] // cmd_filter field used indirectly via ShellExtensions
struct ComposedExtensions {
    cmd_filter: ComposedFilter,
}

impl ShellExtensions for ComposedExtensions {
    type ErrorFormatter = DefaultErrorFormatter;
    type CmdExecFilter = ComposedFilter;
    type SourceFilter = NoOpSourceFilter;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║         Filter Composition Example for brush               ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("This example demonstrates filter composition:");
    println!("  • AuditingCmdFilter: logs commands, blocks 'rm' and 'sudo'");
    println!("  • RateLimitFilter: limits each command to 3 invocations");
    println!();
    println!("Filters are composed using .and_then() - audit runs first,");
    println!("then rate limiting. Post-filters run in reverse order.");
    println!();

    // Create individual filters
    let audit_filter = AuditingCmdFilter::with_blocked(["rm", "sudo"]);
    let rate_filter = RateLimitFilter::new(3);

    // Save reference to audit log for later inspection
    let audit_log = audit_filter.log().clone();

    // ========================================================================
    // Method 1: Using .and_then() (fluent API)
    // ========================================================================
    let composed_filter = audit_filter.and_then(rate_filter);

    // ========================================================================
    // Method 2: Using FilterStack::new() directly (equivalent)
    // ========================================================================
    // let composed_filter = FilterStack::new(
    //     AuditingCmdFilter::with_blocked(["rm", "sudo"]),
    //     RateLimitFilter::new(3),
    // );

    // Create a shell with our composed filter
    let mut shell: Shell<ComposedExtensions> = Shell::builder_with_extensions()
        .cmd_exec_filter(composed_filter)
        .build()
        .await?;

    let source_info = brush_core::SourceInfo::from("<example>");
    let params = shell.default_exec_params();

    // ========================================================================
    // Test 1: Normal command execution (both filters allow)
    // ========================================================================
    println!("─────────────────────────────────────────────────────────────");
    println!("Test 1: echo 'hello' (allowed by both filters)");
    println!("─────────────────────────────────────────────────────────────");
    let _ = shell
        .run_string("echo 'hello'", &source_info, &params)
        .await;

    // ========================================================================
    // Test 2: Blocked by audit filter (rm)
    // ========================================================================
    println!();
    println!("─────────────────────────────────────────────────────────────");
    println!("Test 2: rm file.txt (blocked by AUDIT filter)");
    println!("─────────────────────────────────────────────────────────────");
    let _ = shell.run_string("rm file.txt", &source_info, &params).await;
    // Note: Rate limit filter never sees this because audit filter short-circuits

    // ========================================================================
    // Test 3: Rate limiting in action
    // ========================================================================
    println!();
    println!("─────────────────────────────────────────────────────────────");
    println!("Test 3: Run 'true' 4 times (rate limit is 3)");
    println!("─────────────────────────────────────────────────────────────");
    for i in 1..=4 {
        println!();
        println!("  --- Attempt {i} ---");
        let _ = shell.run_string("true", &source_info, &params).await;
    }

    // ========================================================================
    // Summary
    // ========================================================================
    println!();
    println!("═════════════════════════════════════════════════════════════");
    println!("Audit Log Summary (commands that passed audit filter):");
    println!("═════════════════════════════════════════════════════════════");

    let logged = audit_log.logged_commands();
    if logged.is_empty() {
        println!("  (no commands logged)");
    } else {
        for (i, cmd) in logged.iter().enumerate() {
            println!("  {}. {}", i + 1, cmd);
        }
    }

    println!();
    println!("Key observations:");
    println!("  • 'rm' was blocked by audit filter before rate limit saw it");
    println!("  • 'true' hit rate limit on 4th invocation");
    println!("  • Post-filters run in reverse: rate limit post, then audit post");

    Ok(())
}
