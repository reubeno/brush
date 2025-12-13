# Tuish Architecture

## Overview

Tuish is a TUI-based debugger shell built on brush, using ratatui for the interface. It provides a multi-pane view for inspecting shell state while executing commands.

## Core Components

### Pane System

**PaneRole** - Stable identity for panes regardless of layout position:
```rust
pub enum PaneRole {
    Terminal(u32),    // Multi-instance support (Terminal(0), Terminal(1), ...)
    Completion,       // Singleton
    Environment,      // Singleton
    History,          // Singleton
    Aliases,          // Singleton
    Functions,        // Singleton
    CallStack,        // Singleton
}
```

**Design Principle:** Instance IDs are baked into enum variants only for roles that need multiple instances. This provides:
- Type safety (can't create `Environment(42)`)
- Self-documenting code (`Terminal(0)` vs `Environment`)
- Clean pattern matching (`matches!(role, PaneRole::Terminal(_))`)

### Storage Architecture (Hybrid Approach)

```rust
pub struct AppUI {
    // Special panes with direct access
    primary_terminal: Box<TerminalPane>,
    completion_pane: Box<CompletionPane>,
    
    // General panes stored by role
    panes: HashMap<PaneRole, Box<dyn ContentPane>>,
    tab_order: Vec<PaneRole>,
    
    // UI state
    command_input: CommandInput,
    focused_area: FocusedArea,
}
```

**Why Hybrid?**
- **Special panes** (Terminal, Completion) need direct access for specific operations
- **General panes** (Environment, History, etc.) work through the trait interface
- **No downcasting** required - special panes are concrete types
- **Type-safe API** - `ui.write_to_terminal(data)` just works

### Current Layout

Single tabbed view with:
- 80% of screen: Content area (tabs for different panes)
- 20% of screen: Command input area

Focus can switch between:
- Content panes (via Ctrl+Space)
- Command input area

### Completion Modal

Completion is implemented as a **modal overlay**:
- When triggered (Tab key), replaces content area with completion pane
- Preserves previous focus to restore on dismiss
- Accepts keyboard navigation (arrows, Tab/Shift-Tab) and selection (Enter)
- Dynamic updates: typing continues to filter completions

## Key Abstractions

### ContentPane Trait

All panes implement this trait:
```rust
pub trait ContentPane: Send {
    fn name(&self) -> &'static str;
    fn render(&mut self, frame: &mut Frame, area: Rect);
    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult;
    fn border_title(&self) -> Option<String> { None }
}
```

### Focus Management

- `FocusedArea` enum tracks whether command input or a pane has focus
- Focus cycling via Ctrl+Space
- Focused pane receives keyboard events
- Panes receive `Focused`/`Unfocused` events on focus changes

## Adding New Panes

1. Implement `ContentPane` trait
2. Add role to `PaneRole` enum (if not already present)
3. Create pane instance in `main.rs`
4. Add to `AppUI` via `add_pane(role, pane)`

Example:
```rust
ui.add_pane(
    PaneRole::Environment,
    Box::new(EnvironmentPane::new(&shell))
);
```

## Future Architecture: Flexible Layouts

### Layout Tree (Not Yet Implemented)

When splits are added, layouts will be represented as trees:
```rust
pub enum LayoutNode {
    Pane(PaneRole),           // Leaf: single pane
    HSplit { left, right },   // Horizontal split
    VSplit { top, bottom },   // Vertical split
    Tabs { panes, selected }, // Tabbed (current)
}
```

**Key properties:**
- Recursive rendering (splits divide area and render children)
- Panes referenced by role (stable across layout changes)
- Direct access to special panes still works (role-based lookup)

### Overlay Stack (Partially Implemented)

Completion currently demonstrates the overlay pattern:
- Modal overlays replace or overlay the base layout
- Focus state saved/restored automatically
- Multiple overlay modes possible (full replace, floating, banner, etc.)

Future overlays could include:
- Help screens
- Error details
- Search/filter UI
- Command palette

## Design Decisions

### Why Role-Based Identity?

**Problem:** Need to address panes directly (e.g., write to terminal) even when layout changes.

**Solution:** Semantic roles separate identity from position.
- `PaneRole::Terminal(0)` is always the primary terminal
- Layout can change (tabs → splits) without breaking references
- No dangling IDs or complex lifecycle management

### Why Hybrid Storage?

**Problem:** Some panes need special operations not in `ContentPane` trait.

**Alternatives considered:**
1. Add methods to trait → pollutes interface
2. Downcast trait objects → complex and error-prone
3. Hybrid storage → pragmatic solution

**Result:** Special panes as concrete types, general panes as trait objects.

### Why Instance IDs in Enum Variants?

**Problem:** Need to support multiple terminals in the future.

**Result:** `Terminal(u32)` for multi-instance, plain variants for singletons. This is:
- Idiomatic Rust (enums with data)
- Type-safe (compiler enforces which variants have IDs)
- Self-documenting (code clearly shows which panes support multiple instances)

## Implementation Guidelines

### Adding Pane-Specific Operations

If a pane needs operations beyond `ContentPane`:
1. Is it a special pane? → Add as separate field with concrete type
2. Is it a general pane? → Add method to `ContentPane` with default impl

### Event Routing

Events flow:
1. `AppUI::handle_events()` reads keyboard input
2. Global shortcuts handled first (Ctrl+Q, Ctrl+Space)
3. Completion overlay handled if active
4. Command input handled if focused
5. Focused pane receives events otherwise

### Rendering Pipeline

1. Determine if overlay is active
2. Render appropriate tab bar
3. Render content area:
   - If overlay active: render overlay pane
   - Otherwise: render selected tab content
4. Render command input area
5. Set cursor position if command input focused

## Building and Running

```bash
# Build
cargo build --package tuish

# Run
cargo run --package tuish
```

PTY dimensions are calculated from terminal size (80% of height for content area).
Shell stdout/stderr/stdin are redirected to the PTY for proper terminal emulation.
