# Unified Region Architecture - Refactoring Context

## What We're Doing
Converting CommandInput from a special "area" into a regular region/pane in the layout tree. This unifies the mental model: everything is a region, Ctrl+Space rotates through ALL regions including CommandInput.

## Current State (WORKING, COMPILES)

### Completed Foundation
1. ✅ `PaneKind::CommandInput` added to enum
2. ✅ `PaneEventResult::RequestExecute(String)` and `RequestCompletion` added
3. ✅ `LayoutNode::Tabs` has `splittable: bool` and `closeable: bool` fields
4. ✅ All `LayoutNode::Tabs` construction sites updated with these fields
5. ✅ All pattern matches updated to include `splittable: _, closeable: _`
6. ✅ `CommandInput::render()` renamed to `render_with_cursor()`
7. ✅ `CommandInput` implements `ContentPane` trait
8. ✅ `app_ui.rs` line 477 updated to call `render_with_cursor()`

### Key Files Modified
- `tuish/src/content_pane.rs` - Added `PaneKind::CommandInput` and new `PaneEventResult` variants
- `tuish/src/layout.rs` - Added `splittable`/`closeable` to `Tabs` nodes
- `tuish/src/command_input.rs` - Implements `ContentPane` trait at end of file
- `tuish/src/app_ui.rs` - Updated pattern match at line 1022 for new `PaneEventResult` variants

## Next Steps (Phase A-G)

### Phase A: Refactor AppUI Structure

**Current AppUI fields:**
```rust
pub struct AppUI {
    terminal: DefaultTerminal,
    shell: Arc<Mutex<brush_core::Shell>>,
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    next_pane_id: PaneId,
    primary_terminal: Rc<RefCell<TerminalPane>>,
    completion_pane: Rc<RefCell<CompletionPane>>,
    primary_terminal_id: PaneId,
    completion_pane_id: PaneId,
    layout: LayoutManager,
    focused_region_id: LayoutId,           // ← Will rename to active_region_id
    command_input: CommandInput,            // ← Will REMOVE, becomes a pane
    focused_area: FocusedArea,              // ← Will REMOVE
    pre_completion_focus: Option<FocusedArea>, // ← Change to Option<LayoutId>
    navigation_mode: bool,
    minimize_command_input: bool,
}
```

**Changes needed:**
1. Remove `command_input: CommandInput` field
2. Remove `focused_area: FocusedArea` enum field
3. Rename `focused_region_id` → `active_region_id`
4. Change `pre_completion_focus: Option<FocusedArea>` → `Option<LayoutId>`
5. Add `command_input_pane_id: PaneId` field for special access

### Phase B: Initial Layout Creation

**In `main.rs` or `AppUI::new()`:**

Create CommandInput as first pane:
```rust
let command_input_pane_id = 0;
let command_input = Box::new(CommandInput::new(&shell));
panes.insert(command_input_pane_id, command_input);
next_pane_id = 1;

let terminal_pane_id = 1;
// ... create terminal, other panes starting from ID 2+
```

Create VSplit layout:
```rust
let layout = LayoutManager::new(
    LayoutNode::VSplit {
        id: 0,  // Root split
        top: Box::new(LayoutNode::Tabs {
            id: 1,  // Content region
            panes: vec![terminal_pane_id, env_pane_id, ...],
            selected: 0,
            splittable: true,
            closeable: true,
        }),
        bottom: Box::new(LayoutNode::Tabs {
            id: 2,  // CommandInput region
            panes: vec![command_input_pane_id],
            selected: 0,
            splittable: false,  // Can't split CommandInput region
            closeable: false,   // Can't close CommandInput region
        }),
        split_percent: 80,  // 80% content, 20% input
    }
);

// Set initial active region to CommandInput (region id 2)
active_region_id = 2;
```

### Phase C: Rendering Changes

**In `render()` method:**

REMOVE manual screen split:
```rust
// DELETE THIS:
let main_constraints = if navigation_mode {
    vec![Constraint::Min(10), shrunk_size, Constraint::Length(1)]
} else {
    vec![Constraint::Min(10), command_input_size]
};
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints(main_constraints)
    .split(f.area());
```

REPLACE WITH:
```rust
// Single area for entire layout tree
let full_area = f.area();

// Handle navigation banner separately
let (content_area, nav_area) = if navigation_mode {
    // Reserve 1 line at bottom for banner
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(1)])
        .split(full_area);
    (chunks[0], Some(chunks[1]))
} else {
    (full_area, None)
};

// Render entire layout tree (includes CommandInput region)
let regions = layout.render_layout(content_area);
for (region_id, pane_ids, selected_tab, rect) in regions {
    // ... existing region rendering code ...
}

// Render nav banner if needed
if let Some(nav_area) = nav_area {
    // ... render banner ...
}
```

SPECIAL: Handle cursor for CommandInput pane:
```rust
// After rendering all regions, set cursor if CommandInput is active
if let Some(active_pane_id) = layout.focused_pane() {
    if let Some(pane) = panes.get(&active_pane_id) {
        if matches!(pane.kind(), PaneKind::CommandInput) {
            // Get the CommandInput pane and call render_with_cursor
            // This is tricky - need to downcast or store Rc<RefCell<CommandInput>>
            // Option: Store command_input as Rc<RefCell<CommandInput>> like terminal/completion
        }
    }
}
```

### Phase D: Event Handling

**In `handle_events()`:**

REMOVE special CommandInput handling:
```rust
// DELETE:
_ if matches!(self.focused_area, FocusedArea::CommandInput) => {
    match self.command_input.handle_key(...) { ... }
}
```

REPLACE with unified routing:
```rust
// All non-special keys go to active pane
_ => {
    if let Some(active_pane_id) = self.layout.focused_pane() {
        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
            let result = pane.handle_event(PaneEvent::KeyPress(key.code, key.modifiers));
            match result {
                PaneEventResult::RequestExecute(cmd) => {
                    return Ok(UIEventResult::ExecuteCommand(cmd));
                }
                PaneEventResult::RequestCompletion => {
                    return Ok(UIEventResult::RequestCompletion);
                }
                _ => {}
            }
        }
    }
}
```

### Phase E: Ctrl+Space Rotation

```rust
KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    // Rotate to next region
    let old_pane_id = self.layout.focused_pane();
    
    self.layout.focus_next_region();  // Already cycles through ALL regions
    self.active_region_id = self.layout.focused_node_id().unwrap_or(0);
    
    // Send focus events
    if let Some(old_id) = old_pane_id {
        if let Some(pane) = self.panes.get_mut(&old_id) {
            let _ = pane.handle_event(PaneEvent::Unfocused);
        }
    }
    if let Some(new_id) = self.layout.focused_pane() {
        if let Some(pane) = self.panes.get_mut(&new_id) {
            let _ = pane.handle_event(PaneEvent::Focused);
        }
    }
}
```

### Critical Implementation Note

**CommandInput cursor positioning is special:**

Since `ContentPane::render()` doesn't return cursor position, but CommandInput needs it, we have two options:

**Option 1:** Store CommandInput as `Rc<RefCell<CommandInput>>` like terminal/completion
```rust
struct AppUI {
    command_input_handle: Rc<RefCell<CommandInput>>,  // Direct access
    command_input_pane_id: PaneId,
    // ...
}

// In render, after rendering all regions:
if active region is CommandInput {
    let cursor_pos = self.command_input_handle.borrow_mut().render_with_cursor(...);
    // Set cursor
}
```

**Option 2:** Add cursor position to PaneEventResult
```rust
pub enum PaneEventResult {
    Handled,
    SetCursor(u16, u16),  // New variant
    // ...
}
```

**Recommendation:** Option 1 is cleaner and follows existing pattern (terminal/completion).

## Files to Modify (in order)

1. `tuish/src/app_ui.rs` - Main refactoring
2. `tuish/src/main.rs` - Update initialization
3. `tuish/src/layout.rs` - Add `can_split()` / `can_close()` methods

## Testing Checklist

After each phase, verify:
- [ ] Compiles without errors
- [ ] Renders without panic
- [ ] CommandInput appears at bottom
- [ ] Can type in CommandInput
- [ ] Ctrl+Space rotates through regions
- [ ] n/p navigate regions
- [ ] Tab triggers completion in CommandInput
- [ ] Commands execute correctly
- [ ] Can't split CommandInput region
- [ ] Split operations still work on content regions

## Key Insight

The architecture is now unified: **everything is a region with panes**. The layout tree renders the entire UI. CommandInput is just a region that happens to have `splittable: false` and `closeable: false`.
