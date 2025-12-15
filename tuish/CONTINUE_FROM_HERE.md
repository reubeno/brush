# Continue Unified Region Refactoring From Here

## Current Status
✅ Foundation complete and committed (commit: 5d22fba)
✅ All data structures updated
✅ CommandInput implements ContentPane
✅ Code compiles

🚧 **NEXT:** Complete AppUI refactoring

## What Was Just Done
Modified `tuish/src/app_ui.rs`:
- Removed `FocusedArea` enum
- Changed `focused_region_id` → `active_region_id`
- Removed `command_input: CommandInput` field
- Removed `focused_area: FocusedArea` field
- Removed `minimize_command_input: bool` field
- Added `command_input_handle: Rc<RefCell<CommandInput>>` field
- Added `command_input_pane_id: PaneId` field
- Changed `pre_completion_focus` → `pre_completion_active_region: Option<LayoutId>`

## Immediate Next Steps

### Step 1: Fix AppUI::new() Constructor

Find the `impl AppUI` section with the `new()` method. Make these changes:

```rust
pub fn new(
    shell: &Arc<Mutex<brush_core::Shell>>,
    primary_terminal: Box<TerminalPane>,
    completion_pane: Box<CompletionPane>,
) -> Self {
    let terminal = ratatui::init();

    // IDs: 0=command_input, 1=primary_terminal, 2=completion, 3+=others
    let command_input_pane_id = 0;
    let primary_terminal_id = 1;
    let completion_pane_id = 2;
    let next_pane_id = 3;

    // Create CommandInput
    let command_input = CommandInput::new(shell);
    let command_input_rc = Rc::new(RefCell::new(command_input));

    // Wrap special panes
    let primary_terminal_rc = Rc::new(RefCell::new(*primary_terminal));
    let completion_pane_rc = Rc::new(RefCell::new(*completion_pane));

    // Store all panes
    let mut panes = HashMap::new();
    panes.insert(
        command_input_pane_id,
        Box::new(RcRefCellPaneWrapper::new(command_input_rc.clone())) as Box<dyn ContentPane>,
    );
    panes.insert(
        primary_terminal_id,
        Box::new(RcRefCellPaneWrapper::new(primary_terminal_rc.clone())) as Box<dyn ContentPane>,
    );
    panes.insert(
        completion_pane_id,
        Box::new(RcRefCellPaneWrapper::new(completion_pane_rc.clone())) as Box<dyn ContentPane>,
    );

    // Create VSplit layout: content region (80%) + command input region (20%)
    let layout = LayoutManager::new(
        LayoutNode::VSplit {
            id: 0,  // Root split node
            top: Box::new(LayoutNode::Tabs {
                id: 1,  // Content region
                panes: vec![primary_terminal_id],  // Start with just terminal
                selected: 0,
                splittable: true,
                closeable: true,
            }),
            bottom: Box::new(LayoutNode::Tabs {
                id: 2,  // CommandInput region
                panes: vec![command_input_pane_id],
                selected: 0,
                splittable: false,  // Can't split command input
                closeable: false,   // Can't close command input
            }),
            split_percent: 80,
        }
    );

    // Start with CommandInput region active (region id = 2)
    let active_region_id = 2;

    Self {
        terminal,
        shell: shell.clone(),
        panes,
        next_pane_id,
        primary_terminal: primary_terminal_rc,
        completion_pane: completion_pane_rc,
        command_input_handle: command_input_rc,
        primary_terminal_id,
        completion_pane_id,
        command_input_pane_id,
        layout,
        active_region_id,
        pre_completion_active_region: None,
        navigation_mode: false,
    }
}
```

### Step 2: Compile and Fix Errors

Run: `cargo build --package tuish 2>&1 | grep "error\[" | head -20`

This will show all uses of:
- `focused_area` → replace with active_region_id checks
- `command_input` → replace with command_input_handle.borrow_mut()
- `minimize_command_input` → remove (handle via disabled state)
- `FocusedArea::` → remove enum usage

### Step 3: Fix render() Method

The render method needs major changes. Key points:

**Remove manual split:**
- Delete the `main_constraints` calculation
- Delete the `chunks` split for command input

**New structure:**
```rust
pub fn render(&mut self) -> Result<(), std::io::Error> {
    let active_region_id = self.active_region_id;
    let navigation_mode = self.navigation_mode;
    
    self.terminal.draw(|f| {
        // Split for nav banner if needed
        let (content_area, nav_area) = if navigation_mode {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(1)])
                .split(f.area());
            (chunks[0], Some(chunks[1]))
        } else {
            (f.area(), None)
        };

        // Render entire layout tree (includes CommandInput region)
        // ... existing region rendering code ...
        
        // Special: Set cursor if CommandInput is active
        if let Some((region_id, pane_id)) = self.layout.root().focused_pane() {
            if region_id == active_region_id && pane_id == self.command_input_pane_id {
                if let Some(cursor_pos) = self.command_input_handle.borrow_mut()
                    .render_with_cursor(f, /* need rect */) 
                {
                    if !navigation_mode {
                        f.set_cursor_position(cursor_pos);
                    }
                }
            }
        }

        // Render nav banner if present
        if let Some(nav_rect) = nav_area {
            // ... render banner ...
        }
    })?;

    Ok(())
}
```

**Challenge:** Getting the rect for CommandInput pane requires tracking it during region rendering.

**Solution:** Store rects during rendering:
```rust
let mut pane_rects: HashMap<PaneId, Rect> = HashMap::new();

for (region_id, pane_ids, selected_tab, rect) in regions {
    // ... render region ...
    // Store the rect for the selected pane
    if selected_tab < pane_ids.len() {
        let pane_id = pane_ids[selected_tab];
        pane_rects.insert(pane_id, inner_rect);
    }
}

// Now we can get CommandInput's rect
if let Some(&cmd_rect) = pane_rects.get(&self.command_input_pane_id) {
    if let Some(cursor_pos) = self.command_input_handle.borrow_mut()
        .render_with_cursor(f, cmd_rect) 
    {
        // Set cursor
    }
}
```

### Step 4: Fix Event Handling

Find `handle_events()` method. Major changes:

**Remove completion active special case code** - it checks focused_area

**Replace CommandInput key handling:**
```rust
// OLD:
_ if matches!(self.focused_area, FocusedArea::CommandInput) => {
    match self.command_input.handle_key(key.code, key.modifiers) {
        // ...
    }
}

// NEW:
_ => {
    // Route to active pane
    if let Some(active_pane_id) = self.layout.focused_pane() {
        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
            let result = pane.handle_event(
                crate::content_pane::PaneEvent::KeyPress(key.code, key.modifiers)
            );
            match result {
                PaneEventResult::RequestExecute(cmd) => {
                    return Ok(UIEventResult::ExecuteCommand(cmd));
                }
                PaneEventResult::RequestCompletion => {
                    return Ok(UIEventResult::RequestCompletion);
                }
                PaneEventResult::Handled => {}
                _ => {}
            }
        }
    }
}
```

**Fix Ctrl+Space:**
```rust
KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    // Unfocus current pane
    if let Some(old_pane_id) = self.layout.focused_pane() {
        if let Some(pane) = self.panes.get_mut(&old_pane_id) {
            let _ = pane.handle_event(PaneEvent::Unfocused);
        }
    }

    // Rotate to next region
    self.layout.focus_next_region();
    self.active_region_id = self.layout.focused_node_id().unwrap_or(0);

    // Focus new pane
    if let Some(new_pane_id) = self.layout.focused_pane() {
        if let Some(pane) = self.panes.get_mut(&new_pane_id) {
            let _ = pane.handle_event(PaneEvent::Focused);
        }
    }
}
```

### Step 5: Fix Helper Methods

**set_focus_to_command_input():**
```rust
fn set_focus_to_command_input(&mut self) {
    // Unfocus current
    if let Some(pane_id) = self.layout.focused_pane() {
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            let _ = pane.handle_event(PaneEvent::Unfocused);
        }
    }
    
    // Focus command input region (id = 2)
    self.layout.set_focused_node(2);  // CommandInput region ID
    self.active_region_id = 2;
    
    // Focus the pane
    if let Some(pane) = self.panes.get_mut(&self.command_input_pane_id) {
        let _ = pane.handle_event(PaneEvent::Focused);
    }
}
```

**Remove set_focus_to_next_pane_or_area()** - replaced by Ctrl+Space rotation

### Step 6: Fix run() Method

Changes needed:
1. Remove `command_input.disable()` / `enable()` calls
2. Instead, disable/enable the CommandInput pane directly
3. Remove `command_input.try_refresh().await` call
4. Handle command execution from `RequestExecute` event result

### Step 7: Fix Navigation Mode

All n/p/i handlers need updating:
- Change `focused_area = ContentArea` → use `active_region_id`
- 'i' key should call `set_focus_to_command_input()`
- Remove all `FocusedArea::` references

### Step 8: Test!

```bash
cargo run --package tuish
```

Verify:
1. CommandInput appears at bottom (20% height)
2. Can type in it
3. Pressing Enter executes command
4. Ctrl+Space rotates: Content region → CommandInput → Content
5. Ctrl+B → 'i' focuses CommandInput
6. Tab in CommandInput triggers completion

## Common Errors to Expect

1. **"field not found"** - forgot to update a struct access
2. **"no method named X"** - using old method name
3. **"pattern does not match"** - FocusedArea still being used
4. **"cannot borrow as mutable"** - need to use .borrow_mut() for Rc<RefCell<>>
5. **"moved value"** - forgot to clone an Arc or Rc

## Key Files

- `tuish/src/app_ui.rs` - Main file (90% of changes)
- `tuish/src/main.rs` - Minimal changes (just `add_pane()` calls)
- `tuish/REFACTOR_CONTEXT.md` - Detailed phase descriptions

## Estimated Time

2-3 hours for complete refactoring + testing

## If Stuck

Refer to:
1. `REFACTOR_CONTEXT.md` for detailed phase breakdown
2. `UNIFIED_REGION_PROGRESS.md` for architecture overview
3. Git history: `git show 5d22fba` for foundation work

Good luck! The architecture is sound, just need to connect the wires.
