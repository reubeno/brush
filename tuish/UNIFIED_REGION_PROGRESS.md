# Unified Region Architecture - Implementation Progress

## Goal
Treat CommandInput as just another region in the layout tree, eliminating the special `FocusedArea` concept.

## Completed ✅

### 1. Data Model Changes
- ✅ Added `PaneKind::CommandInput` enum variant
- ✅ Added `PaneEventResult::RequestExecute(String)` 
- ✅ Added `PaneEventResult::RequestCompletion`
- ✅ Added `splittable: bool` to `LayoutNode::Tabs`
- ✅ Added `closeable: bool` to `LayoutNode::Tabs`
- ✅ Updated all `LayoutNode::Tabs` construction sites
- ✅ Updated all pattern matches for `Tabs` nodes

### 2. CommandInput as ContentPane
- ✅ Renamed `CommandInput::render()` → `render_with_cursor()`
- ✅ Implemented `ContentPane` trait for `CommandInput`
  - Maps `PaneEvent::KeyPress` → `handle_key()`
  - Returns `PaneEventResult::RequestExecute` for commands
  - Returns `PaneEventResult::RequestCompletion` for Tab
  - Handles Focused/Unfocused events

### 3. Compilation Status
- ✅ All code compiles successfully
- ✅ No warnings or errors

## Remaining Work 🚧

### Phase A: Refactor AppUI Structure

**Current Structure:**
```rust
pub struct AppUI {
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    layout: LayoutManager,
    focused_area: FocusedArea,          // ❌ TO REMOVE
    focused_region_id: LayoutId,
    command_input: CommandInput,        // ❌ TO REMOVE (becomes a pane)
    // ...
}
```

**Target Structure:**
```rust
pub struct AppUI {
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    layout: LayoutManager,
    active_region_id: LayoutId,         // ✅ Points to ANY region
    command_input_pane_id: PaneId,      // ✅ For special access
    // ...
}
```

**Steps:**
1. Add CommandInput as a pane to `panes` HashMap
2. Remove `command_input: CommandInput` field
3. Remove `focused_area: FocusedArea` field  
4. Add `active_region_id: LayoutId` field
5. Store `command_input_pane_id: PaneId` for special access

### Phase B: Update Initial Layout

**Current:**
```rust
// Single Tabs region with all content panes
LayoutManager::new_tabs(vec![terminal_id, env_id, ...], 0)
```

**Target:**
```rust
// VSplit with content region (top 80%) and command input region (bottom 20%)
LayoutManager::new(
    LayoutNode::VSplit {
        id: 0,
        top: Box::new(LayoutNode::Tabs {
            id: 1,
            panes: vec![terminal_id, env_id, history_id, ...],
            selected: 0,
            splittable: true,
            closeable: true,
        }),
        bottom: Box::new(LayoutNode::Tabs {
            id: 2,
            panes: vec![command_input_id],
            selected: 0,
            splittable: false,  // ✅ CommandInput region can't be split
            closeable: false,   // ✅ CommandInput region can't be closed
        }),
        split_percent: 80,
    }
)
```

**Steps:**
1. Create CommandInput pane before other panes
2. Wrap initial layout in VSplit
3. Set bottom region as non-splittable, non-closeable
4. Set initial focus to command input region

### Phase C: Update Rendering Logic

**Current Rendering:**
- Splits screen into content area + command input area
- Renders layout tree in content area
- Renders CommandInput separately at bottom

**Target Rendering:**
- Render entire layout tree (includes CommandInput region)
- No special cases for CommandInput
- Tab bar automatically hidden when region has 1 pane ✅ Already works!

**Steps:**
1. Remove manual screen split logic
2. Let layout tree render everything
3. Handle cursor positioning for CommandInput pane specially

### Phase D: Update Focus/Event Handling

**Current:**
- `focused_area` decides if CommandInput or content gets events
- Separate logic for CommandInput key handling

**Target:**
- `active_region_id` points to current region
- All events routed through layout tree to active pane
- Special handling for `PaneEventResult::RequestExecute`

**Steps:**
1. Replace all `focused_area` checks with `active_region_id`
2. Route all key events through active pane's `handle_event()`
3. Handle `RequestExecute` and `RequestCompletion` results
4. Remove special CommandInput key handling code

### Phase E: Update Ctrl+Space Behavior

**Current:**
- Toggles between `ContentArea` and `CommandInput`
- Only 2 states

**Target:**
- Rotates through ALL regions in tree (including CommandInput)
- Skips disabled regions
- Natural progression: Region1 → Region2 → ... → CommandInput → Region1

**Steps:**
1. Get all regions from layout tree
2. Find current active region index
3. Move to next region (with wraparound)
4. Skip disabled regions

### Phase F: Update Navigation Mode

**Current:**
- n/p navigate between content regions only
- Special handling for CommandInput

**Target:**
- n/p navigate between ALL regions (including CommandInput)
- No special cases

**Steps:**
1. Already uses `layout.focus_next_region()` ✅
2. Just need to ensure CommandInput region is included ✅
3. Update focused region to active region terminology

### Phase G: Handle Split/Close Protection

**Current:**
- No protection

**Target:**
- Check `splittable` before allowing split
- Check `closeable` before allowing close

**Steps:**
1. Add `can_split()` method to `LayoutManager`
2. Add `can_close()` method to `LayoutManager`
3. Check before performing operations
4. Show feedback if operation not allowed

## Testing Plan

1. **Verify CommandInput as Region**
   - CommandInput appears at bottom
   - Can receive focus
   - Handles input correctly
   - Returns commands

2. **Verify Ctrl+Space Rotation**
   - Rotates through all regions
   - Includes CommandInput in rotation
   - Wraps around correctly

3. **Verify Split Protection**
   - Can't split CommandInput region
   - Can split content regions
   - Can't close CommandInput region

4. **Verify n/p Navigation**
   - Works with CommandInput region
   - Cycles through all regions

5. **Verify Tab Key**
   - In CommandInput: triggers completion
   - In content regions: cycles tabs

## Migration Strategy

**Recommended Approach:** Incremental with testing at each step

1. Phase A → Test compilation
2. Phase B → Test initial layout rendering
3. Phase C → Test full rendering
4. Phase D → Test event routing
5. Phase E → Test Ctrl+Space
6. Phase F → Test navigation mode
7. Phase G → Test protection

**Estimated Time:** 2-3 hours for full implementation + testing

## Current Status

✅ Foundation complete (Phases complete: data model, ContentPane impl)
🚧 Next: Phase A (Refactor AppUI structure)

The hard parts are done! The remaining work is mostly mechanical refactoring with clear steps.
