# Refactoring Plan: Clean Layout Architecture

## Goal
Pure layout tree architecture where:
- All panes (including Terminal, Completion) stored uniformly
- Layout tree contains tab groups and splits
- Tab groups can contain multiple panes
- Focus can move between split regions
- Panes can be moved between regions

## Current Problems

### 1. Hybrid Storage
```rust
// ❌ Current: Split storage
primary_terminal: Box<TerminalPane>
completion_pane: Box<CompletionPane>
panes: HashMap<PaneRole, Box<dyn ContentPane>>  // Only one per role!
```

**Issue**: Can't have multiple terminals or move panes between regions.

### 2. Layout Tree Design
```rust
// ❌ Current: Pane references role, not instance
Pane { id: LayoutId, role: PaneRole }

// ❌ Current: Tabs is top-level, competes with splits
Tabs { panes: Vec<PaneRole>, selected: usize }
```

**Issue**: 
- Can't distinguish between Terminal(0) instance 1 vs instance 2
- Tabs should be a region type, not compete with HSplit/VSplit

## Target Architecture

### 1. Unified Pane Storage
```rust
/// Unique identifier for pane instances
pub type PaneId = usize;

pub struct AppUI {
    // Unified storage - ALL panes here
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    
    // Mapping for special pane access
    primary_terminal_id: PaneId,
    completion_pane_id: PaneId,
    
    // Layout tree references PaneIds
    layout: LayoutManager,
    
    // Focus tracking
    focused_region_id: LayoutId,
}
```

### 2. Redesigned Layout Tree
```rust
pub enum LayoutNode {
    /// Tabbed region containing multiple panes
    Tabs {
        id: LayoutId,
        panes: Vec<PaneId>,      // ✅ References instances, not roles
        selected: usize,
    },
    /// Horizontal split (left | right)
    HSplit {
        id: LayoutId,
        left: Box<LayoutNode>,
        right: Box<LayoutNode>,
        split_percent: u16,
    },
    /// Vertical split (top / bottom)  
    VSplit {
        id: LayoutId,
        top: Box<LayoutNode>,
        bottom: Box<LayoutNode>,
        split_percent: u16,
    },
}
```

**Key Changes:**
- ✅ Remove `Pane` variant - tabs are the leaf nodes
- ✅ Tabs contain `Vec<PaneId>` not `Vec<PaneRole>`
- ✅ Can have multiple panes per role (e.g., 3 terminals in one tab group)
- ✅ Tabs can exist anywhere in tree (not just root)

### 3. Example Layout Tree
```
HSplit (50/50)
├─ Tabs [Terminal#0, Terminal#1, Environment]  ← selected: 0 (showing Terminal#0)
└─ VSplit (50/50)
   ├─ Tabs [History, CallStack]                ← selected: 0 (showing History)
   └─ Tabs [Aliases, Functions]                ← selected: 1 (showing Functions)
```

**Renders as:**
```
┌─────────────────┬──────────────┐
│  Terminal #0    │  History     │
│  (tabs: T0, T1, │              │
│   Environment)  ├──────────────┤
│                 │  Functions   │
│                 │  (showing 1) │
└─────────────────┴──────────────┘
```

## Implementation Steps

### Phase 1: Pane Storage Refactor (Breaking Change)
**File**: `src/app_ui.rs`

1. **Add PaneId type**:
```rust
pub type PaneId = usize;
```

2. **Replace storage**:
```rust
pub struct AppUI {
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    next_pane_id: PaneId,
    
    // Special pane IDs for direct access
    primary_terminal_id: PaneId,
    completion_pane_id: PaneId,
    
    layout: LayoutManager,
    focused_region_id: LayoutId,
}
```

3. **Update constructor**:
```rust
pub fn new(
    shell: &Arc<Mutex<brush_core::Shell>>,
    primary_terminal: Box<TerminalPane>,
    completion_pane: Box<CompletionPane>,
    pty: Arc<crate::pty::Pty>,
) -> Self {
    let mut panes = HashMap::new();
    
    // Assign IDs
    let primary_terminal_id = 0;
    let completion_pane_id = 1;
    let next_pane_id = 2;
    
    panes.insert(primary_terminal_id, primary_terminal as Box<dyn ContentPane>);
    panes.insert(completion_pane_id, completion_pane as Box<dyn ContentPane>);
    
    // Start with single tab containing primary terminal
    let layout = LayoutManager::new_tabs(vec![primary_terminal_id], 0);
    
    Self {
        terminal: ratatui::init(),
        shell: shell.clone(),
        panes,
        next_pane_id,
        primary_terminal_id,
        completion_pane_id,
        layout,
        command_input: CommandInput::new(shell),
        focused_region_id: 0,
        pty_handle: pty,
        // ... other fields
    }
}
```

4. **Update `add_pane`**:
```rust
pub fn add_pane(&mut self, pane: Box<dyn ContentPane>) -> PaneId {
    let pane_id = self.next_pane_id;
    self.next_pane_id += 1;
    self.panes.insert(pane_id, pane);
    
    // Add to focused region's tab group
    self.layout.add_pane_to_region(self.focused_region_id, pane_id);
    
    pane_id
}
```

### Phase 2: Layout Tree Refactor (Breaking Change)
**File**: `src/layout.rs`

1. **Update LayoutNode**:
```rust
pub enum LayoutNode {
    Tabs {
        id: LayoutId,
        panes: Vec<PaneId>,  // Changed from Vec<PaneRole>
        selected: usize,
    },
    HSplit { /* unchanged */ },
    VSplit { /* unchanged */ },
    // Remove Pane variant
}
```

2. **Update `render_layout`**:
```rust
pub fn render_layout(&self, area: Rect) -> Vec<(LayoutId, Vec<PaneId>, Rect)> {
    // Returns: (region_id, pane_ids_in_region, rect)
    match self {
        Self::Tabs { id, panes, selected } => {
            vec![(*id, vec![panes[*selected]], area)]
        }
        // ... handle splits recursively
    }
}
```

3. **Add region operations**:
```rust
impl LayoutManager {
    /// Adds a pane to the specified region (must be a Tabs node)
    pub fn add_pane_to_region(&mut self, region_id: LayoutId, pane_id: PaneId) -> bool;
    
    /// Removes a pane from its region
    pub fn remove_pane(&mut self, pane_id: PaneId) -> bool;
    
    /// Moves a pane to a different region
    pub fn move_pane(&mut self, pane_id: PaneId, target_region_id: LayoutId) -> bool;
    
    /// Splits the focused region, moving selected pane to new side
    pub fn split_region_vertical(&mut self) -> bool;
    pub fn split_region_horizontal(&mut self) -> bool;
    
    /// Gets all regions (tab groups) in the tree
    pub fn get_all_regions(&self) -> Vec<LayoutId>;
    
    /// Cycles focus to next/previous region
    pub fn focus_next_region(&mut self);
    pub fn focus_prev_region(&mut self);
}
```

### Phase 3: Rendering Integration
**File**: `src/app_ui.rs`

1. **Update render logic**:
```rust
// Get all visible regions
let regions = self.layout.render_layout(content_area);

for (region_id, pane_ids, rect) in regions {
    let is_focused = region_id == self.focused_region_id;
    
    // Render tab bar if multiple panes in region
    if pane_ids.len() > 1 {
        // Render tabs at top of rect
        // Adjust rect for content area
    }
    
    // Render active pane
    let pane_id = pane_ids[0]; // Selected pane
    if let Some(pane) = self.panes.get_mut(&pane_id) {
        let border_style = if is_focused { /* focused */ } else { /* unfocused */ };
        // Render with border
        pane.render(f, inner_rect);
    }
}
```

### Phase 4: Navigation Updates
**File**: `src/app_ui.rs`

1. **Update navigation mode**:
```rust
// In handle_events():
KeyCode::Char('v') if self.navigation_mode => {
    self.layout.split_region_vertical();
}
KeyCode::Char('h') if self.navigation_mode => {
    self.layout.split_region_horizontal();
}
KeyCode::Char('n') if self.navigation_mode => {
    self.layout.focus_next_region();
}
KeyCode::Char('p') if self.navigation_mode => {
    self.layout.focus_prev_region();
}
KeyCode::Tab if self.navigation_mode => {
    // Cycle tabs within focused region
    self.layout.cycle_tabs_in_region(self.focused_region_id);
}
```

## Migration Strategy

### Option A: Clean Break (Recommended)
1. Create new branch from current `tuish`
2. Implement Phase 1-4 in order
3. Update all tests
4. Merge when complete

**Pros**: Clean, no technical debt
**Cons**: Larger initial work

### Option B: Incremental
1. Add PaneId alongside existing storage
2. Gradually migrate callers
3. Remove old storage

**Pros**: Smaller PRs
**Cons**: Confusing intermediate state

## Recommendation

**Go with Option A (Clean Break)** because:
1. Current code is already in `tuish` branch (experimental)
2. No external users to break
3. Architecture is clear and well-defined
4. Cleaner git history

## Estimated Effort

- **Phase 1** (Pane Storage): ~2-3 hours
  - Update AppUI struct and constructor
  - Update add_pane and pane access patterns
  - Update main.rs pane creation

- **Phase 2** (Layout Tree): ~3-4 hours  
  - Update LayoutNode enum
  - Rewrite layout operations
  - Update tests

- **Phase 3** (Rendering): ~2-3 hours
  - Update render loop
  - Handle tab bars in regions
  - Update border rendering

- **Phase 4** (Navigation): ~1-2 hours
  - Update key handlers
  - Test focus management

**Total**: ~8-12 hours for complete clean implementation

## Next Immediate Steps

1. Review this plan - any changes needed?
2. Start Phase 1: PaneId storage refactor
3. Get basic rendering working with new storage
4. Continue through phases

Let me know if you want me to start implementing this clean architecture!
