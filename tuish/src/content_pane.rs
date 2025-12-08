//! Content pane abstraction for tuish.
//!
//! This module defines the `ContentPane` trait that allows different types of content
//! to be displayed in tabbed panes without knowledge of screen positioning.

#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;

/// Events that can be sent to a content pane
#[derive(Debug, Clone)]
pub enum PaneEvent {
    /// A key was pressed while this pane was focused
    KeyPress(KeyCode, KeyModifiers),
    /// The pane gained focus
    Focused,
    /// The pane lost focus
    Unfocused,
    /// The pane's render area has changed
    Resized { width: u16, height: u16 },
}

/// Result of handling a pane event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneEventResult {
    /// Event was handled, no special action needed
    Handled,
    /// Event was not handled (propagate to parent)
    NotHandled,
    /// Request to close this pane
    RequestClose,
}

/// A content pane that can be displayed in a tab.
///
/// Implementations handle their own state, scrolling, and rendering,
/// but are unaware of screen positioning.
/// Trait for content panes that can be displayed in tabs.
///
/// When a pane is focused, it receives all keyboard input except global shortcuts
/// (like Ctrl+Q and Ctrl+Space). Each pane decides how to handle its input.
pub trait ContentPane {
    /// Enable downcasting to concrete types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Returns the display name for this pane (shown in tab bar)
    fn name(&self) -> &str;

    /// Renders the pane content to the given area.
    ///
    /// The area provided is the inner content area (borders are handled externally).
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect);

    /// Handles an event directed at this pane.
    ///
    /// Returns `PaneEventResult` indicating how the event was handled.
    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult;

    /// Called when the pane becomes visible (tab selected).
    ///
    /// Allows panes to refresh or initialize state.
    fn on_show(&mut self) {}

    /// Called when the pane becomes hidden (tab deselected).
    ///
    /// Allows panes to clean up or pause updates.
    fn on_hide(&mut self) {}

    /// Returns whether this pane wants to receive all keyboard input.
    ///
    /// If true, the pane gets raw keyboard events (useful for terminal emulation).
    /// If false, only navigation keys are forwarded.
    fn wants_all_input(&self) -> bool {
        false
    }

    /// Returns whether this pane is scrollable.
    fn is_scrollable(&self) -> bool {
        false
    }
}
