# Completion Architecture & Modal Pane Pattern

## Current Implementation

The completion pane is currently special-cased in `AppUI`:
- Stored separately as `completion_pane: Option<Box<CompletionPane>>`
- Has dedicated `is_active()` check to override normal rendering
- Event handling has special branches for completion mode
- Focus is saved/restored with `pre_completion_focus`

## The Problem

This pattern will repeat if we want other "modal" or "overlay" behaviors:
- Error/warning dialogs
- Help overlay
- Command palette
- Search/filter UI
- Confirmation prompts

## Proposed: Modal Pane Trait

### Design Option 1: Modal Pane Trait Extension

```rust
/// Marker trait for panes that can be shown modally (overlaying other content)
pub trait ModalPane: ContentPane {
    /// Returns whether this modal is currently active
    fn is_modal_active(&self) -> bool;
    
    /// Activates the modal with optional data
    fn activate_modal(&mut self, data: Option<Box<dyn std::any::Any>>);
    
    /// Deactivates the modal
    fn deactivate_modal(&mut self);
    
    /// Returns whether this modal allows passthrough to underlying panes
    /// (e.g., for semi-transparent overlays)
    fn allows_passthrough(&self) -> bool {
        false
    }
}
```

Usage in AppUI:
```rust
pub struct AppUI {
    // ...
    /// Modal panes that can overlay normal content
    modal_panes: Vec<Box<dyn ModalPane>>,
    /// Currently active modal pane index
    active_modal: Option<usize>,
    /// Focus state before modal was activated
    pre_modal_focus: Option<FocusedArea>,
}

impl AppUI {
    fn render(&mut self) -> Result<(), std::io::Error> {
        // Check for active modal
        let active_modal = self.active_modal
            .and_then(|idx| self.modal_panes.get_mut(idx))
            .filter(|modal| modal.is_modal_active());
        
        if let Some(modal) = active_modal {
            // Render modal instead of (or atop) normal panes
            modal.render(f, content_area);
        } else {
            // Normal pane rendering
        }
    }
}
```

### Design Option 2: Render Layer Enum

```rust
pub enum RenderLayer {
    /// Normal tabbed panes
    Normal,
    /// Overlay that replaces content area
    ModalOverlay,
    /// Floating overlay (could be positioned anywhere)
    FloatingOverlay,
}

pub trait ContentPane {
    // ...existing methods...
    
    /// Returns the render layer for this pane
    fn render_layer(&self) -> RenderLayer {
        RenderLayer::Normal
    }
    
    /// Returns whether this pane is currently requesting to be shown
    fn wants_display(&self) -> bool {
        true // Normal panes always want to display
    }
}
```

Then AppUI groups panes by layer:
```rust
pub struct AppUI {
    /// All content panes (normal and modal)
    panes: Vec<Box<dyn ContentPane>>,
    // ...
}

impl AppUI {
    fn render(&mut self) -> Result<(), std::io::Error> {
        // Find the highest-priority pane that wants display
        let modal_pane = self.panes.iter_mut()
            .filter(|p| p.wants_display())
            .find(|p| matches!(p.render_layer(), RenderLayer::ModalOverlay));
        
        if let Some(modal) = modal_pane {
            // Render modal
        } else {
            // Render normal tabbed panes
        }
    }
}
```

### Design Option 3: Activation Stack (Most Flexible)

```rust
pub struct AppUI {
    /// Normal panes in tabs
    normal_panes: Vec<Box<dyn ContentPane>>,
    /// Stack of activated overlays (top = currently visible)
    overlay_stack: Vec<OverlayState>,
}

struct OverlayState {
    /// The pane being shown as overlay
    pane: Box<dyn ContentPane>,
    /// Focus state before this overlay activated
    previous_focus: FocusedArea,
    /// Optional data passed to overlay
    context: Option<Box<dyn std::any::Any>>,
}

impl AppUI {
    /// Pushes an overlay onto the stack
    pub fn push_overlay(&mut self, pane: Box<dyn ContentPane>, context: Option<Box<dyn std::any::Any>>) {
        self.overlay_stack.push(OverlayState {
            pane,
            previous_focus: self.focused_area,
            context,
        });
    }
    
    /// Pops the current overlay and restores focus
    pub fn pop_overlay(&mut self) {
        if let Some(state) = self.overlay_stack.pop() {
            self.focused_area = state.previous_focus;
        }
    }
    
    fn render(&mut self) -> Result<(), std::io::Error> {
        // Render topmost overlay if any, otherwise normal panes
        if let Some(overlay) = self.overlay_stack.last_mut() {
            overlay.pane.render(f, content_area);
        } else {
            // Normal rendering
        }
    }
}
```

## Recommendation: Option 3 (Activation Stack)

**Why:**
- Most general - supports multiple overlay types
- Stack semantics match user mental model (modal on top of modal)
- Clean separation: normal panes vs. transient overlays
- Easy to extend for future overlay types
- Context passing enables data flow (e.g., error details, search query)

**Implementation for Completion:**
```rust
// In AppUI::handle_events when Tab is pressed:
UIEventResult::RequestCompletion => {
    let completions = /* ... get completions ... */;
    
    if completions.len() == 1 {
        // Auto-accept
    } else {
        // Create completion pane and push as overlay
        let mut comp_pane = CompletionPane::new(&self.shell);
        comp_pane.set_completions(completions);
        self.push_overlay(Box::new(comp_pane), None);
    }
}

// When Enter is pressed in completion mode:
if let Some(overlay) = self.overlay_stack.last_mut() {
    if let Some(comp_pane) = overlay.pane.as_any_mut().downcast_mut::<CompletionPane>() {
        if let Some(completion) = comp_pane.selected_completion() {
            // Apply completion
        }
    }
}
self.pop_overlay(); // Restore previous state
```

## Future Use Cases Enabled

1. **Help Overlay**: `Ctrl+?` → Push help pane → `Esc` → Pop
2. **Error Details**: Command fails → Push error pane with stack trace → `Enter` → Pop
3. **Command Palette**: `Ctrl+P` → Push searchable command list → Select → Pop
4. **Nested Modals**: Confirmation prompt on top of settings dialog
5. **Split View**: Multiple overlays side-by-side (stack → grid)

## Migration Path

1. **Phase 1** (now): Keep current implementation, document pattern
2. **Phase 2**: Introduce `OverlayStack` alongside current code
3. **Phase 3**: Migrate completion to use overlay stack
4. **Phase 4**: Add new overlay types (help, errors, etc.)

## Notes

- CompletionPane should still implement ContentPane (same trait)
- "Overlay" is just a rendering/lifecycle concept, not a type hierarchy
- Consider adding `as_any()` to ContentPane for downcasting in overlay handlers
