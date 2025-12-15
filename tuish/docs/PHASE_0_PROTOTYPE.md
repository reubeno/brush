# Phase 0 Prototype: "Lite X-Ray" (No Hooks Required)

## Goal
Ship a working demo **this weekend** using only existing brush infrastructure.

## What We Can Build TODAY

### Using What Already Exists:
- ✅ `brush-parser` — Parse script into AST
- ✅ `Shell::env()` — Query variables
- ✅ `Shell::call_stack()` — Get call stack (already used in CallStackPane)
- ✅ `tracing::debug!` — Existing trace points in brush-core
- ✅ PTY + Terminal — Output display (already working)
- ✅ Pane system — Tabs, rendering (already working)

### Architecture: Polling + Tracing Events

```
┌─────────────────────────────────────────────────────────────┐
│                    tuish Process                             │
│                                                              │
│  ┌──────────────┐         ┌─────────────────────────────┐  │
│  │ Source Pane  │         │   Script Execution Task     │  │
│  │ (static)     │         │   (tokio::spawn)            │  │
│  └──────────────┘         │                             │  │
│                           │  shell.run_string()         │  │
│  ┌──────────────┐         │       ↓                     │  │
│  │ Variables    │◄────────┤  Periodic poll:             │  │
│  │ (polled)     │ 100ms   │  - shell.env().variables()  │  │
│  └──────────────┘         │  - shell.call_stack()       │  │
│                           └─────────────────────────────┘  │
│  ┌──────────────┐                     ↓                    │
│  │ Call Stack   │◄────────── tracing events ──────────────┤
│  │ (inferred)   │         (function entry/exit)            │
│  └──────────────┘                                          │
│                                                              │
│  ┌──────────────┐                                          │
│  │ Terminal     │◄────────── PTY output ──────────────────┤
│  │ (live)       │         (already working)                │
│  └──────────────┘                                          │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation (2-3 days)

### File 1: `tuish/src/script_watch_mode.rs` (NEW)

```rust
//! Script watch mode without execution hooks (Phase 0)

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use brush_core::{ExecutionParameters, SourceInfo};
use tokio::sync::Mutex;

use crate::source_pane::SourceCodePane;
use crate::app_ui::AppUI;

/// Lightweight script visualizer using polling
pub struct ScriptWatchMode {
    /// Reference to the UI
    ui: AppUI,
    /// Source code pane
    source_pane: SourceCodePane,
    /// Shell instance
    shell: Arc<Mutex<brush_core::Shell>>,
    /// Last known variable values (for change detection)
    last_vars: HashMap<String, String>,
}

impl ScriptWatchMode {
    pub fn new(ui: AppUI, shell: &Arc<Mutex<brush_core::Shell>>) -> Self {
        Self {
            ui,
            source_pane: SourceCodePane::new(),
            shell: shell.clone(),
            last_vars: HashMap::new(),
        }
    }
    
    /// Run a script with live visualization (no hooks, polling-based)
    pub async fn run(&mut self, script_path: &Path) -> Result<()> {
        // 1. Load script into source pane
        let script_content = std::fs::read_to_string(script_path)?;
        self.source_pane.load_script(&script_path.to_path_buf())?;
        
        // 2. Start script execution in background
        let shell = self.shell.clone();
        let content = script_content.clone();
        let source_info = SourceInfo::new(
            brush_core::SourceLocation::File(script_path.into())
        );
        
        let exec_task = tokio::spawn(async move {
            let mut shell = shell.lock().await;
            let params = ExecutionParameters::default();
            shell.run_string(content, &source_info, &params).await
        });
        
        // 3. Poll shell state while script runs
        let mut poll_interval = tokio::time::interval(Duration::from_millis(100));
        
        loop {
            // Check if script finished
            if exec_task.is_finished() {
                match exec_task.await? {
                    Ok(result) => {
                        let exit_code: u8 = (&result.exit_code).into();
                        self.ui.write_to_terminal(
                            format!("\n[Script completed with exit code {}]\n", exit_code)
                                .as_bytes()
                        );
                    }
                    Err(e) => {
                        self.ui.write_to_terminal(
                            format!("\n[Script failed: {}]\n", e).as_bytes()
                        );
                    }
                }
                break;
            }
            
            // Poll shell state (non-blocking)
            if let Ok(shell) = self.shell.try_lock() {
                self.update_state(&shell);
            }
            
            // Render UI
            self.ui.render()?;
            
            // Handle user input
            if self.handle_input().await? {
                break; // User requested exit
            }
            
            poll_interval.tick().await;
        }
        
        Ok(())
    }
    
    fn update_state(&mut self, shell: &brush_core::Shell) {
        // Get current variables
        let env = shell.env();
        
        // Detect variable changes
        for (name, var) in env.variables() {
            let value = var.value().to_string();
            
            if let Some(old_value) = self.last_vars.get(name) {
                if old_value != &value {
                    // Variable changed!
                    tracing::debug!("Variable changed: {} = {}", name, value);
                    // TODO: Highlight in UI
                }
            } else {
                // New variable
                tracing::debug!("Variable set: {} = {}", name, value);
            }
            
            self.last_vars.insert(name.clone(), value);
        }
        
        // Note: Call stack is already updated via existing CallStackPane
        // which queries shell.call_stack() during render
    }
    
    async fn handle_input(&mut self) -> Result<bool> {
        // Delegate to AppUI for input handling
        // Returns true if user wants to exit
        match self.ui.handle_events()? {
            crate::app_ui::UIEventResult::RequestExit => Ok(true),
            _ => Ok(false),
        }
    }
}
```

### File 2: `tuish/src/source_pane.rs` (NEW - Simplified)

```rust
//! Source code pane for script visualization

use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};

pub struct SourceCodePane {
    /// Path to script file
    script_path: Option<PathBuf>,
    /// Source lines
    lines: Vec<String>,
    /// Scroll offset
    scroll_offset: usize,
}

impl SourceCodePane {
    pub fn new() -> Self {
        Self {
            script_path: None,
            lines: Vec::new(),
            scroll_offset: 0,
        }
    }
    
    pub fn load_script(&mut self, path: &PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        self.lines = content.lines().map(String::from).collect();
        self.script_path = Some(path.clone());
        Ok(())
    }
}

impl ContentPane for SourceCodePane {
    fn name(&self) -> &'static str {
        "Source"
    }
    
    fn kind(&self) -> PaneKind {
        PaneKind::Source
    }
    
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if self.lines.is_empty() {
            let empty = Paragraph::new("No script loaded")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(empty, area);
            return;
        }
        
        // Build rows with line numbers
        let rows: Vec<Row> = self.lines
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(area.height as usize)
            .map(|(idx, line_text)| {
                Row::new(vec![
                    Cell::from(format!("{:4}", idx + 1))
                        .style(Style::default().fg(Color::DarkGray)),
                    Cell::from(line_text.as_str()),
                ])
            })
            .collect();
        
        let table = Table::new(
            rows,
            [Constraint::Length(5), Constraint::Percentage(100)]
        ).style(Style::default().fg(Color::White));
        
        frame.render_widget(table, area);
    }
    
    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        match event {
            PaneEvent::KeyPress(crossterm::event::KeyCode::Up, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(crossterm::event::KeyCode::Down, _) => {
                self.scroll_offset = self.scroll_offset
                    .saturating_add(1)
                    .min(self.lines.len().saturating_sub(1));
                PaneEventResult::Handled
            }
            _ => PaneEventResult::NotHandled,
        }
    }
}
```

### File 3: Modify `tuish/src/main.rs`

```rust
// Add to imports
use clap::Parser;

#[derive(Parser)]
#[command(name = "tuish")]
struct Args {
    /// Script to watch (optional - runs REPL if not provided)
    script: Option<PathBuf>,
    
    /// Watch mode (visualize script execution)
    #[arg(long)]
    watch: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    // Build shell
    let shell = brush_core::Shell::builder()
        .interactive(args.script.is_none()) // Only interactive if no script
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .external_cmd_leads_session(true)
        .shell_name(String::from("tuish"))
        .shell_product_display_str(String::from("tuish"))
        .build()
        .await?;
    
    let shell = Arc::new(tokio::sync::Mutex::new(shell));
    
    // Check if we're running a script
    if let Some(script_path) = args.script {
        // Script mode
        run_script_mode(&shell, &script_path).await
    } else {
        // Interactive REPL mode (existing behavior)
        run_interactive_mode(&shell).await
    }
}

async fn run_script_mode(
    shell: &Arc<Mutex<brush_core::Shell>>,
    script_path: &Path,
) -> Result<()> {
    // Create UI (simplified - no PTY needed for script mode initially)
    let mut ui = create_script_mode_ui(shell).await?;
    
    // Create script watch mode
    let mut watch_mode = ScriptWatchMode::new(ui, shell);
    
    // Run script with visualization
    watch_mode.run(script_path).await
}

async fn run_interactive_mode(
    shell: &Arc<Mutex<brush_core::Shell>>
) -> Result<()> {
    // Existing interactive mode code
    // ... (same as current main.rs)
}
```

### File 4: Add to `tuish/src/content_pane.rs`

```rust
pub enum PaneKind {
    Terminal,
    Environment,
    History,
    Aliases,
    Functions,
    CallStack,
    Source, // NEW
}
```

---

## Demo Flow (60 seconds)

### Setup:
```bash
# Create a simple buggy script
cat > demo.sh << 'EOF'
#!/bin/bash
# Deploy script that has a subtle bug

echo "Starting deployment..."

# BUG: Reads DATABASE_URL but it's not set
DB_HOST="${DATABASE_URL:-localhost}"
DB_PORT=5432

echo "Connecting to $DB_HOST:$DB_PORT..."

if [ "$DB_HOST" = "localhost" ]; then
  echo "ERROR: DB_HOST defaulted to localhost!"
  exit 1
fi

echo "Deployment successful"
EOF

chmod +x demo.sh
```

### Demo:
```bash
# Run with tuish
cargo run --package tuish -- demo.sh

# UI shows:
# ┌─────────────────────────────────────────────┐
# │ [Source✓] [Variables] [Call Stack] [Output]│
# ├─────────────────────────────────────────────┤
# │ Source:              │ Variables:           │
# │  1 │ #!/bin/bash      │  DATABASE_URL (unset)│
# │  2 │ # Deploy script  │  DB_HOST  localhost  │◄─ BUG!
# │  3 │                  │  DB_PORT  5432       │
# │  4 │ echo "Start..."  │                      │
# │  5 │                  │                      │
# │  6 │ # BUG: Reads...  │                      │
# │  7 │ DB_HOST="${DA... │                      │
# ├─────────────────────────────────────────────┤
# │ Output:                                      │
# │ Starting deployment...                       │
# │ Connecting to localhost:5432...              │
# │ ERROR: DB_HOST defaulted to localhost!       │
# └─────────────────────────────────────────────┘

# Bug is OBVIOUS: DATABASE_URL is unset, so DB_HOST defaults to localhost
```

---

## What This Proves

### ✅ Immediate Value:
- See variable values during execution
- Understand script state evolution
- Find bugs faster than logs alone
- No modifications to script needed

### ✅ Technical Feasibility:
- Works with existing brush infrastructure
- No hooks required (yet)
- Can ship THIS WEEKEND

### ✅ User Feedback:
- "Is this valuable?"
- "What's missing?"
- "Would you use this?"

### ⚠️ Limitations (vs Full X-Ray):
- No line-by-line highlighting (don't know current line)
- Variable updates are delayed (100ms polling, not instant)
- No step mode (can't pause execution)
- No command tracing (can't see what command is executing)

But these limitations are **acceptable for Phase 0**. Users will still find it valuable!

---

## Testing Plan

### Manual Testing:
```bash
# Test 1: Simple script with variables
echo 'count=0; for i in {1..5}; do count=$((count+i)); done; echo $count' > test1.sh
cargo run --package tuish -- test1.sh
# Expect: Variables pane shows count changing: 0→1→3→6→10→15

# Test 2: Script with functions
cat > test2.sh << 'EOF'
outer() { inner "arg1"; }
inner() { echo "Got: $1"; }
outer
EOF
cargo run --package tuish -- test2.sh
# Expect: Call stack shows outer→inner

# Test 3: Buggy script (demo.sh from above)
cargo run --package tuish -- demo.sh
# Expect: See DB_HOST defaulting to localhost
```

### Automated Testing:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_script_watch_mode_loads_script() {
        let shell = create_test_shell().await;
        let mut watch = ScriptWatchMode::new(/* ... */);
        
        let script = Path::new("test_script.sh");
        std::fs::write(script, "echo 'test'").unwrap();
        
        // Should load without error
        assert!(watch.source_pane.load_script(&script.into()).is_ok());
    }
    
    #[tokio::test]
    async fn test_variable_change_detection() {
        // Test that we detect when variables change
        // ...
    }
}
```

---

## Migration Path to Phase 1 (Full X-Ray)

Once brush-core has execution hooks:

### Changes needed:
1. Replace polling with hook events
2. Add line highlighting (from SourceInfo in hooks)
3. Add command text display
4. Add step mode (pause/resume)

### What stays the same:
- Source pane rendering
- Variables pane (just different data source)
- Call stack pane
- Terminal pane
- UI layout

**Estimated effort:** 1-2 days (most UI code reused)

---

## Success Criteria for Phase 0

Before moving to Phase 1, we need:

1. ✅ **Demo works end-to-end** (can run script and see state)
2. ✅ **At least 3 users say "this is useful"** (Discord/GitHub feedback)
3. ✅ **No major bugs** (doesn't crash, renders correctly)
4. ✅ **Performance acceptable** (< 100ms latency in variable updates)

If these are met → proceed to Phase 1 (Full X-Ray with hooks)
If not → reconsider approach

---

## Timeline

### Day 1 (Saturday): Core Implementation
- [ ] Create `source_pane.rs` (2 hours)
- [ ] Create `script_watch_mode.rs` (3 hours)
- [ ] Modify `main.rs` for script mode (1 hour)
- [ ] Basic integration testing (1 hour)

### Day 2 (Sunday): Polish & Demo
- [ ] Fix bugs from testing (2 hours)
- [ ] Create demo scripts (1 hour)
- [ ] Record demo video (1 hour)
- [ ] Write blog post / Discord announcement (1 hour)
- [ ] Share with community (ongoing)

### Day 3-7 (Week 1): Iterate
- [ ] Gather feedback
- [ ] Fix critical bugs
- [ ] Improve UI based on feedback
- [ ] Document limitations

**By end of Week 1:** Decision point for Phase 1

---

## Questions Before Starting

1. **Should we add `--watch` flag or just detect script argument?**
   - Proposal: Just detect script argument (simpler UX)
   - `tuish` → REPL mode
   - `tuish script.sh` → Watch mode

2. **Where should SourceCodePane live?**
   - New file `tuish/src/source_pane.rs`
   - Follows existing pattern (environment_pane.rs, etc.)

3. **Should we poll faster than 100ms?**
   - 100ms = 10 FPS for variable updates
   - Too slow? Too fast?
   - Let users decide?

4. **What about PTY in script mode?**
   - Keep it (script output goes to PTY → terminal pane)
   - Or remove it (simpler, but lose interactive capability)?
   - Recommendation: Keep it (reuse existing code)

---

## Bottom Line

**Phase 0 can ship THIS WEEKEND** with:
- 2-3 days of implementation
- No brush-core changes required
- Immediate user feedback
- Clear path to Phase 1

**Key insight:** Don't let perfect be the enemy of good. Ship Lite mode now, upgrade to Full mode when hooks are ready.
