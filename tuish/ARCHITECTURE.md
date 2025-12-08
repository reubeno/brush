# Tuish Architecture Refactoring Plan

## Current State Analysis

The current `ratatui_backend.rs` has the following issues:

1. **Tight coupling**: Terminal and environment pane logic are hardcoded in draw_ui()
2. **State sprawl**: `env_scroll_offset`, `tab_titles`, etc. scattered in the backend
3. **No abstraction**: Adding new pane types requires modifying draw_ui() and handle_events()
4. **Position awareness**: Panes know about screen positioning (tab indices, etc.)

## New Architecture - PARTIALLY IMPLEMENTED

### Core Abstraction: `ContentPane` Trait ✅

```rust
pub trait ContentPane {
    fn name(&self) -> &str;                              // Tab label
    fn render(&mut self, frame: &mut Frame, area: Rect); // Render content
    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult;
    fn wants_all_input(&self) -> bool;                   // Terminal vs scrollable pane
    fn is_scrollable(&self) -> bool;
}
```

### Pane Implementations ✅

1. **TerminalPane** (`terminal_pane.rs`) ✅
   - Wraps PTY parser + writer
   - Forwards all keyboard to PTY
   - Renders using tui_term::PseudoTerminal

2. **EnvironmentPane** (`environment_pane.rs`) ✅
   - Displays env vars in a table
   - Handles scrolling (Up/Down/PageUp/PageDown/Home/End)
   - Updates via `update_variables()`

3. **Future panes**: Jobs, History, Files, etc.

### Modified Backend Structure ⏳ IN PROGRESS

**Status**: Struct updated, but draw_ui() and handle_events() need complete refactor

**Challenge**: Ratatui's closure-based drawing API makes it difficult to call pane.render() inside terminal.draw()
because we need `&mut self.panes` but terminal.draw() already borrows `&mut self`.

**Solution Options**:

1. Use unsafe cell/refcell for panes
2. Extract pane rendering to separate methods called before draw()
3. Use ratatui's StatefulWidget pattern
4. Manually call pane.render() by splitting borrow scopes

### Current Backend (Partial)

```rust
pub struct RatatuiInputBackend {
    terminal: DefaultTerminal,
    panes: Vec<Box<dyn ContentPane>>,  // ✅ Done
    pty_stdin/stdout/stderr,           // ✅ Done  
    input_buffer: String,              // ✅ Done
    cursor_pos: usize,                 // ✅ Done
    focused_area: FocusedArea,         // ✅ Done (was focused_pane)
}
```

## Remaining Work

### Critical Path to Working Refactor

1. **Fix draw_ui() borrowing issue** ⚠️ BLOCKED
   - Problem: Can't call `pane.render(frame, area)` inside `terminal.draw(|frame| {...})`
   - Need to either:
     a) Use `RefCell<Vec<Box<dyn ContentPane>>>`
     b) Render panes to buffers before terminal.draw()
     c) Redesign to work with ratatui's widget system

2. **Complete draw_ui() refactor**
   - Remove hardcoded if/else for tab 0/1
   - Use `panes.iter()` and `pane.name()` for tab titles
   - Call `pane.render()` for selected pane

3. **Complete handle_events() refactor**
   - Remove all references to `FocusedPane::Tab(0)` / `Tab(1)`
   - Use `FocusedArea::Pane(idx)` with dynamic pane count
   - Dispatch events to `pane.handle_event()` instead of hardcoding
   - Remove direct PTY forwarding (now in TerminalPane)

4. **Update main.rs**
   - Get EnvironmentPane via `backend.get_pane_mut(1)`
   - Call `update_variables()` on it
   - Update `draw_ui()` call signature (no more env_vars param)

## Recommended Approach

**Option A: Complete Refactor Now** (High risk, lots of code)

- Rewrite entire ratatui_backend.rs
- ~500+ lines of changes
- High chance of subtle bugs

**Option B: Incremental with Feature Flag** (Safer)

- Keep old code working
- Build new architecture in parallel
- Switch over when ready
- Can test both

**Option C: Simplify Architecture** (Pragmatic)

- Keep some coupling in draw_ui()
- Use ContentPane for extensibility, not isolation  
- Panes still render themselves, but backend orchestrates
- Easier migration path

## Current Status

✅ Trait definitions complete and compiling
✅ TerminalPane implementation complete
✅ EnvironmentPane implementation complete  
✅ RatatuiInputBackend struct updated with panes Vec
✅ Panes created in new() method
⏳ draw_ui() partially refactored (borrowing issue)
❌ handle_events() not yet updated
❌ main.rs not yet updated

## Next Steps - DECISION NEEDED

Before proceeding with full refactor, decide:

1. Which refactoring approach? (A, B, or C above)
2. Should we invest in solving the borrowing issue, or adjust the architecture?
3. Timeline: Fast iteration vs robust design?

**Recommendation**: Start with Option C (pragmatic) - get it working quickly, then refine.
