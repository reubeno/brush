# Script Execution X-Ray Mode: UX & Architecture Design

## Executive Summary

This document proposes adding **Script Execution Mode** to `tuish` that provides "x-ray vision" into shell script execution. While the current REPL mode enables interactive debugging, this new mode lets developers **watch scripts run** with unprecedented visibility into shell state, execution flow, and real-time debugging.

**Core Value:** Transform shell script debugging from "printf debugging and guesswork" to "step through with live inspection" — like gdb/lldb for bash.

---

## 🎯 User Experience Vision

### The Pain We're Solving

Shell scripts are uniquely difficult to debug:
- ❌ No visibility into variable state during execution
- ❌ No clear indication of which line is executing  
- ❌ Complex control flow is opaque (functions, loops, conditionals)
- ❌ Can't pause and inspect without modifying script
- ❌ Exit codes and errors provide minimal context
- ❌ `set -x` output is overwhelming and hard to parse

### The Solution: Three Execution Modes

#### 1. **Watch Mode** (Live Visualization)
```bash
tuish script.sh
```

**Experience:**
- Script executes at normal speed
- Terminal shows output in real-time
- Source pane highlights currently executing line
- State panes (vars, call stack) update live
- **Like a profiler, but for shell execution flow**

**Use Cases:**
- Understand unfamiliar scripts quickly
- Verify script does what documentation says
- Educational: teaching bash to students
- CI/CD debugging: see exactly what failed

#### 2. **Step Mode** (Interactive Debugging)
```bash
tuish --step script.sh
# or press 'S' during watch mode
```

**Experience:**
- Script pauses before each command
- Press `Space`/`Enter` to execute next line
- Full state inspection at each step
- Set breakpoints on specific lines
- **Like gdb for bash scripts**

**Controls:**
- `Space` / `Enter` — Execute next line
- `C` — Continue to end (or next breakpoint)
- `B` — Toggle breakpoint on current line
- `W` — Switch to watch mode (auto-run)
- `Q` — Quit execution

**Use Cases:**
- Find exact line where bug occurs
- Understand why conditional took unexpected path
- Debug complex parameter expansions
- Learn how bash features work

#### 3. **Replay Mode** (Post-Mortem Analysis)
```bash
tuish --replay execution.log
```

**Experience:**
- Scrub through recorded execution like a video
- Seek to any point in time
- Inspect full state at any moment
- Search for specific events
- **Like Chrome DevTools Timeline for bash**

**Controls:**
- `←` / `→` — Step backward/forward
- `Shift+←` / `→` — Jump to prev/next function call
- `Home` / `End` — Jump to start/end
- `/` — Search for command or variable change
- `Space` — Play/pause

**Use Cases:**
- Debug production failures without reproducing
- "What was the value of X when Y failed?"
- Share execution traces with bug reports
- Create training materials from expert executions

---

## 🎨 UI Design

### Layout: Script Watch/Step Mode

```
┌──────────────────────────────────────────────────────────────────┐
│ [Source✓] [Variables] [Call Stack] [Trace] [Output]             │ ← Tabs
├──────────────────────────────────────────────────────────────────┤
│                                                                    │
│  SOURCE (60% width)            │ STATE INSPECTOR (40%)           │
│  ───────────────────            │ ────────────────────           │
│  script.sh                      │ Environment Variables:         │
│                                 │  ┌──────────────────────────┐  │
│   18 │ for file in *.log; do   │  │ count    = 42            │  │
│   19 │   echo "Processing..."   │  │ file     = "app.log"     │  │
│   20 │   count=$((count + 1))   │  │ total    = 156           │  │
│ ► 21 │   process_log "$file"    │  │ HOSTNAME = "prod-01"     │  │
│   22 │ done                     │  └──────────────────────────┘  │
│   23 │                          │                                │
│   24 │ function process_log {   │ Call Stack:                    │
│   25 │   local log=$1           │  ► process_log("app.log")     │
│   26 │   grep ERROR "$log"      │    └─ for loop (line 18)      │
│   27 │ }                        │       └─ main                  │
│                                 │                                │
├──────────────────────────────────────────────────────────────────┤
│ TERMINAL OUTPUT                                       [RUNNING]  │
│ Processing app.log...                                            │
│ ERROR: Connection timeout on line 42                             │
│ Processing system.log...                                         │
│ █                                                                │
├──────────────────────────────────────────────────────────────────┤
│ ⚡ WATCH MODE │ Line 21/87 │ 2.3s │ [W]atch [S]tep [Q]uit       │
└──────────────────────────────────────────────────────────────────┘
```

### Key UI Components

#### Source Code Pane
**Purpose:** Show script with execution position

**Features:**
- Line numbers (gutter)
- Current line indicator: `►` or highlight background
- Breakpoint markers: `●` (red)
- Auto-scroll to follow execution
- Manual scroll when paused
- Syntax highlighting (future enhancement)

**Interactions:**
- `Up`/`Down` — Scroll source
- `B` — Toggle breakpoint on current line
- `G` — Go to line number (modal input)

#### State Inspector (Right Sidebar)
**Purpose:** Live view of shell state

**Sub-sections:**
- **Variables:** Show all variables, highlight recent changes
- **Call Stack:** Show function call hierarchy with args
- **Aliases:** Show defined aliases
- **Functions:** List defined functions

**Interactions:**
- Collapsible sections
- Search/filter variables
- Click to inspect complex values (arrays)

#### Terminal Output Pane
**Purpose:** Show script's stdout/stderr

**Features:**
- Full VT100 emulation (via tui-term)
- Scrollback buffer
- Clear indication of running vs paused
- Exit code display when complete

#### Control Bar (Footer)
**Purpose:** Show status and available controls

**Information:**
- Current mode (Watch/Step/Replay)
- Current line number / total lines
- Elapsed time
- Exit code (when complete)

**Controls:**
- Contextual hints based on mode
- Visual indicators for state (PAUSED, RUNNING, DONE)

---

## 🏗️ Architecture Design

### 1. Execution Mode State

```rust
// In tuish/src/execution_mode.rs (new file)

use std::collections::HashSet;
use std::path::PathBuf;
use brush_core::SourceLocation;

/// Execution mode for tuish
#[derive(Debug, Clone)]
pub enum ExecutionMode {
    /// Interactive REPL (current behavior)
    Interactive,
    
    /// Watch script execution with live updates
    ScriptWatch {
        script_path: PathBuf,
        record_trace: Option<PathBuf>,
    },
    
    /// Step through script with manual control
    ScriptStep {
        script_path: PathBuf,
        breakpoints: HashSet<SourceLocation>,
        record_trace: Option<PathBuf>,
    },
    
    /// Replay recorded execution trace
    Replay {
        trace_path: PathBuf,
    },
}

impl ExecutionMode {
    pub fn is_script_mode(&self) -> bool {
        !matches!(self, Self::Interactive)
    }
    
    pub fn script_path(&self) -> Option<&PathBuf> {
        match self {
            Self::ScriptWatch { script_path, .. } 
            | Self::ScriptStep { script_path, .. } => Some(script_path),
            _ => None,
        }
    }
}
```

### 2. Execution Observer Pattern

**Goal:** Hook into brush-core execution without tight coupling

```rust
// In brush-core/src/observability.rs (new file)

use crate::{ExecutionResult, SourceInfo};
use std::sync::Arc;

/// Observer that receives execution lifecycle events
pub trait ExecutionObserver: Send + Sync {
    /// Called before executing a command
    fn on_command_start(&mut self, ctx: &CommandContext) {
        let _ = ctx;
    }
    
    /// Called after command completes
    fn on_command_end(&mut self, ctx: &CommandContext, result: &ExecutionResult) {
        let _ = (ctx, result);
    }
    
    /// Called when entering a function
    fn on_function_enter(&mut self, name: &str, args: &[String]) {
        let _ = (name, args);
    }
    
    /// Called when exiting a function
    fn on_function_exit(&mut self, name: &str, exit_code: u8) {
        let _ = (name, exit_code);
    }
    
    /// Called when a variable is set
    fn on_variable_set(&mut self, name: &str, value: &str, scope: VarScope) {
        let _ = (name, value, scope);
    }
}

/// Context provided to observer callbacks
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub source_info: SourceInfo,
    pub command_text: String,
    pub expanded_text: Option<String>,
    pub call_stack_depth: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum VarScope {
    Local,
    Global,
    Environment,
}

/// Extension to Shell for observer management
pub trait ShellObservabilityExt {
    fn set_execution_observer(&mut self, observer: Option<Arc<dyn ExecutionObserver>>);
}
```

**Integration Points in brush-core:**

1. **Command execution** (`brush-core/src/commands.rs` or similar):
   ```rust
   async fn execute_command(&mut self, cmd: &Command) -> Result<ExecutionResult> {
       // Notify observer before execution
       if let Some(observer) = &self.execution_observer {
           let ctx = CommandContext {
               source_info: cmd.source_info.clone(),
               command_text: cmd.to_string(),
               expanded_text: None, // Could expand here
               call_stack_depth: self.call_stack.len(),
           };
           observer.on_command_start(&ctx);
       }
       
       // Execute command
       let result = self.execute_command_impl(cmd).await?;
       
       // Notify observer after execution
       if let Some(observer) = &self.execution_observer {
           observer.on_command_end(&ctx, &result);
       }
       
       Ok(result)
   }
   ```

2. **Variable assignment** (`brush-core/src/env.rs`):
   ```rust
   pub fn set_variable(&mut self, name: &str, value: &str) {
       self.variables.insert(name.to_string(), value.to_string());
       
       if let Some(observer) = &self.execution_observer {
           observer.on_variable_set(name, value, VarScope::Global);
       }
   }
   ```

### 3. Tuish Execution Observer

```rust
// In tuish/src/execution_observer.rs (new file)

use brush_core::{CommandContext, ExecutionObserver, ExecutionResult, VarScope};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};

/// Events sent from observer to UI thread
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    CommandStart {
        source_info: SourceInfo,
        command: String,
        expanded: Option<String>,
    },
    CommandEnd {
        exit_code: u8,
    },
    VariableSet {
        name: String,
        value: String,
        scope: VarScope,
    },
    FunctionEnter {
        name: String,
        args: Vec<String>,
    },
    FunctionExit {
        name: String,
        exit_code: u8,
    },
}

pub struct TuishExecutionObserver {
    /// Channel to send events to UI
    event_tx: mpsc::Sender<ExecutionEvent>,
    
    /// Whether execution is paused (step mode)
    paused: Arc<std::sync::atomic::AtomicBool>,
    
    /// Notification for continuing execution
    continue_notify: Arc<Notify>,
    
    /// Breakpoints (line numbers)
    breakpoints: Arc<std::sync::RwLock<HashSet<usize>>>,
}

impl TuishExecutionObserver {
    pub fn new(
        event_tx: mpsc::Sender<ExecutionEvent>,
        paused: bool,
    ) -> Self {
        Self {
            event_tx,
            paused: Arc::new(std::sync::atomic::AtomicBool::new(paused)),
            continue_notify: Arc::new(Notify::new()),
            breakpoints: Arc::new(std::sync::RwLock::new(HashSet::new())),
        }
    }
    
    pub fn pause(&self) {
        self.paused.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    
    pub fn resume(&self) {
        self.continue_notify.notify_one();
    }
    
    pub fn add_breakpoint(&self, line: usize) {
        self.breakpoints.write().unwrap().insert(line);
    }
    
    pub fn remove_breakpoint(&self, line: usize) {
        self.breakpoints.write().unwrap().remove(&line);
    }
    
    async fn check_pause_point(&self, source_info: &SourceInfo) {
        let should_pause = self.paused.load(std::sync::atomic::Ordering::SeqCst)
            || self.is_breakpoint(source_info);
        
        if should_pause {
            // Wait for user to continue
            self.continue_notify.notified().await;
        }
    }
    
    fn is_breakpoint(&self, source_info: &SourceInfo) -> bool {
        if let Some(start) = &source_info.start {
            self.breakpoints.read().unwrap().contains(&start.line)
        } else {
            false
        }
    }
}

impl ExecutionObserver for TuishExecutionObserver {
    fn on_command_start(&mut self, ctx: &CommandContext) {
        // Send event to UI (non-blocking)
        let _ = self.event_tx.try_send(ExecutionEvent::CommandStart {
            source_info: ctx.source_info.clone(),
            command: ctx.command_text.clone(),
            expanded: ctx.expanded_text.clone(),
        });
        
        // Check if we should pause
        // NOTE: This needs to be async-aware in real implementation
        // May need tokio::task::block_in_place or similar
        tokio::runtime::Handle::current().block_on(
            self.check_pause_point(&ctx.source_info)
        );
    }
    
    fn on_command_end(&mut self, _ctx: &CommandContext, result: &ExecutionResult) {
        let exit_code: u8 = (&result.exit_code).into();
        let _ = self.event_tx.try_send(ExecutionEvent::CommandEnd { exit_code });
    }
    
    fn on_variable_set(&mut self, name: &str, value: &str, scope: VarScope) {
        let _ = self.event_tx.try_send(ExecutionEvent::VariableSet {
            name: name.to_string(),
            value: value.to_string(),
            scope,
        });
    }
    
    fn on_function_enter(&mut self, name: &str, args: &[String]) {
        let _ = self.event_tx.try_send(ExecutionEvent::FunctionEnter {
            name: name.to_string(),
            args: args.to_vec(),
        });
    }
    
    fn on_function_exit(&mut self, name: &str, exit_code: u8) {
        let _ = self.event_tx.try_send(ExecutionEvent::FunctionExit {
            name: name.to_string(),
            exit_code,
        });
    }
}
```

### 4. Source Code Pane

```rust
// In tuish/src/source_pane.rs (new file)

use std::collections::HashSet;
use std::path::PathBuf;
use brush_core::SourceInfo;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct SourceCodePane {
    /// Path to the script file
    script_path: Option<PathBuf>,
    
    /// Source code lines
    lines: Vec<String>,
    
    /// Current executing line (1-indexed)
    current_line: Option<usize>,
    
    /// Breakpoints (1-indexed line numbers)
    breakpoints: HashSet<usize>,
    
    /// Scroll offset (for viewport)
    scroll_offset: usize,
    
    /// Table state for rendering
    table_state: TableState,
}

impl SourceCodePane {
    pub fn new() -> Self {
        Self {
            script_path: None,
            lines: Vec::new(),
            current_line: None,
            breakpoints: HashSet::new(),
            scroll_offset: 0,
            table_state: TableState::default(),
        }
    }
    
    /// Load script from file
    pub fn load_script(&mut self, path: &PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        self.lines = content.lines().map(String::from).collect();
        self.script_path = Some(path.clone());
        Ok(())
    }
    
    /// Update current executing line
    pub fn set_current_line(&mut self, source_info: &SourceInfo) {
        if let Some(pos) = &source_info.start {
            self.current_line = Some(pos.line);
            self.scroll_to_line(pos.line);
        }
    }
    
    /// Toggle breakpoint at line
    pub fn toggle_breakpoint(&mut self, line: usize) -> bool {
        if self.breakpoints.contains(&line) {
            self.breakpoints.remove(&line);
            false
        } else {
            self.breakpoints.insert(line);
            true
        }
    }
    
    /// Auto-scroll to keep line visible
    fn scroll_to_line(&mut self, line: usize) {
        // Keep line in middle third of viewport
        let viewport_height = 30; // Will be calculated from render area
        let target_offset = line.saturating_sub(viewport_height / 3);
        self.scroll_offset = target_offset;
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
        
        let viewport_height = area.height as usize;
        
        // Build rows for visible lines
        let rows: Vec<Row> = self.lines
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(viewport_height)
            .map(|(idx, line_text)| {
                let line_num = idx + 1;
                
                // Determine styling
                let is_current = Some(line_num) == self.current_line;
                let has_breakpoint = self.breakpoints.contains(&line_num);
                
                let mut style = Style::default();
                let mut prefix = "  ";
                
                if is_current {
                    style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
                    prefix = "► ";
                } else if has_breakpoint {
                    prefix = "● ";
                    style = style.fg(Color::Red);
                }
                
                Row::new(vec![
                    Cell::from(format!("{:4}", line_num))
                        .style(Style::default().fg(Color::DarkGray)),
                    Cell::from(prefix).style(style),
                    Cell::from(line_text.as_str()),
                ]).style(style)
            })
            .collect();
        
        let widths = [
            Constraint::Length(5),      // Line number
            Constraint::Length(2),       // Indicator (► or ●)
            Constraint::Percentage(100), // Source code
        ];
        
        let table = Table::new(rows, widths)
            .style(Style::default().fg(Color::White));
        
        frame.render_widget(table, area);
    }
    
    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        match event {
            PaneEvent::KeyPress(KeyCode::Char('b'), _) => {
                if let Some(line) = self.current_line {
                    self.toggle_breakpoint(line);
                }
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(KeyCode::Up, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(KeyCode::Down, _) => {
                let max_scroll = self.lines.len().saturating_sub(1);
                self.scroll_offset = self.scroll_offset.saturating_add(1).min(max_scroll);
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(KeyCode::PageUp, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(KeyCode::PageDown, _) => {
                let max_scroll = self.lines.len().saturating_sub(10);
                self.scroll_offset = self.scroll_offset.saturating_add(10).min(max_scroll);
                PaneEventResult::Handled
            }
            _ => PaneEventResult::NotHandled,
        }
    }
}
```

### 5. Modified AppUI for Script Mode

```rust
// Changes to tuish/src/app_ui.rs

impl AppUI {
    pub async fn run(&mut self) -> Result<()> {
        match &self.execution_mode {
            ExecutionMode::Interactive => {
                self.run_interactive().await
            }
            ExecutionMode::ScriptWatch { script_path, record_trace } => {
                self.run_script_mode(script_path, false, record_trace.as_ref()).await
            }
            ExecutionMode::ScriptStep { script_path, breakpoints, record_trace } => {
                self.run_script_mode(script_path, true, record_trace.as_ref()).await
            }
            ExecutionMode::Replay { trace_path } => {
                self.run_replay_mode(trace_path).await
            }
        }
    }
    
    async fn run_script_mode(
        &mut self,
        script_path: &Path,
        step_mode: bool,
        record_trace: Option<&Path>,
    ) -> Result<()> {
        // 1. Load script into source pane
        if let Some(source_pane) = self.get_source_pane_mut() {
            source_pane.load_script(&script_path.to_path_buf())?;
        }
        
        // 2. Set up execution observer
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1000);
        let observer = Arc::new(tokio::sync::Mutex::new(
            TuishExecutionObserver::new(event_tx, step_mode)
        ));
        
        // 3. Register observer with shell
        {
            let mut shell = self.shell.lock().await;
            shell.set_execution_observer(Some(observer.clone()));
        }
        
        // 4. Start script execution in background task
        let shell = self.shell.clone();
        let script_content = std::fs::read_to_string(script_path)?;
        let source_info = SourceInfo::new(SourceLocation::File(script_path.into()));
        
        let execution_task = tokio::spawn(async move {
            let mut shell = shell.lock().await;
            let params = ExecutionParameters::default();
            shell.run_string(script_content, &source_info, &params).await
        });
        
        // 5. Main event loop
        loop {
            // Process execution events from observer
            while let Ok(exec_event) = event_rx.try_recv() {
                self.handle_execution_event(exec_event)?;
            }
            
            // Check if execution finished
            if execution_task.is_finished() {
                match execution_task.await? {
                    Ok(result) => {
                        let exit_code: u8 = (&result.exit_code).into();
                        self.show_completion_message(exit_code);
                        
                        // In step mode, wait for user before exiting
                        if step_mode {
                            self.wait_for_user_exit().await?;
                        }
                    }
                    Err(e) => {
                        self.show_error_message(&format!("Script failed: {}", e));
                    }
                }
                break;
            }
            
            // Render UI
            self.render()?;
            
            // Handle user input
            match self.handle_events()? {
                UIEventResult::RequestExit => break,
                UIEventResult::StepNext => {
                    // Resume execution for one step
                    observer.lock().await.resume();
                }
                UIEventResult::Continue => {
                    // Switch to watch mode (unpause)
                    observer.lock().await.unpause();
                }
                UIEventResult::ToggleBreakpoint => {
                    // Toggle breakpoint on current line
                    if let Some(source_pane) = self.get_source_pane_mut() {
                        if let Some(line) = source_pane.current_line() {
                            if source_pane.toggle_breakpoint(line) {
                                observer.lock().await.add_breakpoint(line);
                            } else {
                                observer.lock().await.remove_breakpoint(line);
                            }
                        }
                    }
                }
                _ => {}
            }
            
            tokio::time::sleep(Duration::from_millis(16)).await; // ~60 FPS
        }
        
        Ok(())
    }
    
    fn handle_execution_event(&mut self, event: ExecutionEvent) -> Result<()> {
        match event {
            ExecutionEvent::CommandStart { source_info, command, .. } => {
                // Update source pane to highlight current line
                if let Some(source_pane) = self.get_source_pane_mut() {
                    source_pane.set_current_line(&source_info);
                }
                
                // Show running command in terminal border
                self.set_running_command(Some(command));
            }
            ExecutionEvent::CommandEnd { exit_code } => {
                // Could display exit code in status bar
                self.last_exit_code = Some(exit_code);
            }
            ExecutionEvent::VariableSet { .. } => {
                // Variables pane auto-updates via shell state query
                // No action needed here
            }
            ExecutionEvent::FunctionEnter { .. } 
            | ExecutionEvent::FunctionExit { .. } => {
                // Call stack pane auto-updates via shell state query
                // No action needed here
            }
        }
        Ok(())
    }
}
```

---

## 📋 Implementation Roadmap

### Phase 1: MVP Watch Mode (2-3 weeks)

**Goal:** Basic script visualization working end-to-end

#### Tasks:
1. **CLI argument parsing** (2 hours)
   - Add `--watch` and `--step` flags to main.rs
   - Add script_path argument
   - Create ExecutionMode enum

2. **ExecutionObserver trait in brush-core** (8 hours)
   - Define trait in new `observability.rs` module
   - Add observer field to Shell struct
   - Insert hooks in command execution path
   - Test with simple println observer

3. **TuishExecutionObserver** (4 hours)
   - Implement observer with event channel
   - Basic pause/resume logic for step mode
   - Event serialization

4. **SourceCodePane** (6 hours)
   - File loading
   - Line rendering with numbers
   - Current line highlighting
   - Breakpoint display

5. **Wire up script mode in AppUI** (8 hours)
   - Modify run() to dispatch by mode
   - Implement run_script_mode()
   - Event loop integration
   - Update source pane from events

**Success Criteria:**
- ✅ Can run `tuish --watch script.sh`
- ✅ Source pane shows script and highlights current line
- ✅ Terminal pane shows script output
- ✅ Variables/call stack update during execution
- ✅ Script completes and shows exit code

### Phase 2: Step Mode & Breakpoints (1-2 weeks)

**Goal:** Interactive debugging with breakpoints

#### Tasks:
1. **Breakpoint management** (4 hours)
   - Toggle breakpoints with 'B' key
   - Check breakpoints in observer
   - Visual indicators in source pane

2. **Step controls** (6 hours)
   - Space/Enter to step next line
   - 'C' to continue execution
   - 'W' to switch to watch mode
   - Status bar showing mode

3. **Pause/resume logic** (4 hours)
   - Block execution at breakpoints
   - Tokio notify for resume signal
   - Handle state transitions

**Success Criteria:**
- ✅ Can run `tuish --step script.sh`
- ✅ Execution pauses at each line
- ✅ Can set/remove breakpoints
- ✅ Can continue to next breakpoint
- ✅ Can switch between watch and step modes

### Phase 3: Polish & UX (1 week)

**Goal:** Professional user experience

#### Tasks:
1. **Syntax highlighting** (6 hours)
   - Use brush-parser for tokens
   - Color keywords, strings, variables
   - Handle edge cases

2. **Enhanced variable display** (4 hours)
   - Show arrays and assoc arrays
   - Highlight recent changes
   - Filter/search capability

3. **Status bar improvements** (3 hours)
   - Show line number, time, exit code
   - Visual indicators (PAUSED, RUNNING)
   - Helpful keybinding hints

4. **Documentation** (4 hours)
   - Update README with examples
   - Add screencast to docs
   - Write tutorial

**Success Criteria:**
- ✅ Source code is syntax highlighted
- ✅ Variables display is rich and informative
- ✅ Status bar is clear and helpful
- ✅ Documentation explains all features

### Phase 4: Replay Mode (2-3 weeks)

**Goal:** Record and replay executions

#### Tasks:
1. **Trace file format** (6 hours)
   - Design JSON schema for events
   - Implement serialization
   - Handle large files efficiently

2. **Recording infrastructure** (4 hours)
   - Add --record flag
   - Write events to file during execution
   - Handle errors gracefully

3. **Replay engine** (10 hours)
   - Load trace file
   - Seekable event stream
   - Reconstruct state at any point
   - Handle incomplete traces

4. **Replay UI controls** (6 hours)
   - Seek forward/backward
   - Play/pause
   - Speed control
   - Timeline scrubber

**Success Criteria:**
- ✅ Can record execution with `--record`
- ✅ Can replay with `--replay trace.log`
- ✅ Can seek to any point in time
- ✅ State is accurately reconstructed

---

## ❓ Open Design Questions

### Q1: How granular should stepping be?

**Options:**
- A) Line-level (execute all commands on a line)
- B) Command-level (execute one command at a time)
- C) AST-node level (step through parser nodes)

**Recommendation:** **B (command-level)** for MVP. Most intuitive for users. Can add "step over" (line) and "step into" (AST) later.

### Q2: How to handle sourced files?

**Challenge:** Scripts often `source` other files.

**Options:**
- A) Show only main script
- B) Tab per sourced file
- C) Inline sourced content

**Recommendation:** **B (tab per file)**. Add new source pane when file is sourced. SourceInfo already tracks file paths.

### Q3: How to handle long-running scripts?

**Challenge:** Scripts that run for minutes/hours.

**Options:**
- A) Sampling mode (observe every Nth command)
- B) Detach/reattach (background the execution)
- C) Checkpointing (save state periodically)

**Recommendation:** **A (sampling)** for Phase 3, **C (checkpointing)** for Phase 4. Always record trace to file for long scripts.

### Q4: Performance overhead?

**Challenge:** Observer callbacks could slow execution.

**Options:**
- A) Optimize observer to be zero-cost when disabled
- B) Compile-time feature flag
- C) Sampling mode to reduce overhead

**Recommendation:** **A + C**. Use `Option<Arc<dyn Observer>>` so check is just a pointer null check. Add sampling if needed.

### Q5: How to handle interactive scripts?

**Challenge:** Scripts that read from stdin.

**Options:**
- A) PTY handles it transparently (current)
- B) Show input modal in step mode
- C) Redirect to special input pane

**Recommendation:** **A** for MVP (just works). Consider **B** for better UX in step mode (pause, show input prompt, resume).

---

## 🎯 Success Metrics

### User Experience Metrics
- **Time to understand script:** < 10 minutes (was 30+ min)
- **Time to find bug:** < 15 minutes (was hours)
- **User confidence:** High visibility = high confidence

### Adoption Metrics
- GitHub stars, Discord engagement
- Real-world usage (trace files shared, bug reports with recordings)
- Educational adoption (tutorials, courses)

### Technical Metrics
- **Performance overhead:** < 20% in watch mode
- **Memory footprint:** < 100MB typical
- **Startup time:** < 500ms

---

## 🚀 Marketing & Positioning

### Tagline
**"X-ray vision for shell scripts. Debug bash like you debug code."**

### Key Messages
1. **See execution flow in real-time** — Variables, call stack, line-by-line
2. **Step through like gdb** — Breakpoints, pause, inspect state
3. **Record and replay** — Debug production failures, share traces

### Demo Script
```bash
# Show a buggy deploy script
cat deploy.sh

# Run in step mode
tuish --step deploy.sh

# [Show in screencast:]
# - Step through lines
# - See DB_HOST is empty (bug!)
# - Set breakpoint at problem line
# - Continue to breakpoint
# - Fix bug

# Narrator: "Found bug in 30 seconds. No print statements needed."
```

### Target Audiences
- **DevOps/SRE:** Debug CI/CD, deployment scripts
- **Educators:** Teach bash with visualizations
- **OSS Maintainers:** Debug build scripts
- **Security Researchers:** Analyze suspicious scripts

---

## 🎬 Conclusion

This design transforms tuish from an experimental TUI into a **revolutionary debugging tool**. By adding execution observability to brush-core and leveraging tuish's existing pane architecture, we create a uniquely powerful experience for shell script development.

**Next Steps:**
1. Review with community (Discord, GitHub Discussion)
2. Prototype Phase 1 (2-3 weeks)
3. Gather early feedback
4. Iterate based on real-world usage

**The vision:** Make shell scripts as debuggable as compiled code.

---

**Version:** 1.0  
**Date:** 2025-12-13  
**Status:** Design Proposal — Ready for Review
