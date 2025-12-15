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
    /// Request to execute a command
    RequestExecute(String),
    /// Request to trigger completion
    RequestCompletion,
}

/// Kinds of content panes available in tuish.
pub enum PaneKind {
    /// A terminal pane
    Terminal,
    /// An environment variables pane
    Environment,
    /// A command history pane
    History,
    /// A shell aliases pane
    Aliases,
    /// A shell functions pane
    Functions,
    /// A call stack pane
    CallStack,
    /// The command input pane
    CommandInput,
}

/// A content pane that can be displayed in a tab.
///
/// Implementations handle their own state, scrolling, and rendering,
/// but are unaware of screen positioning.
/// Trait for content panes that can be displayed in tabs.
///
/// When a pane is focused, it receives all keyboard input except global shortcuts
/// (like Ctrl+Q and Ctrl+Space). Each pane decides how to handle its input.
pub trait ContentPane: Send {
    /// Returns the display name for this pane (shown in tab bar)
    fn name(&self) -> &'static str;

    /// Returns the kind of the pane.
    fn kind(&self) -> PaneKind;

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

    /// Returns an optional title to display in the pane's border.
    ///
    /// The default implementation returns `None`, meaning no title is displayed.
    /// Panes can override this to show dynamic status information.
    fn border_title(&self) -> Option<String> {
        None
    }

    /// Returns whether this pane is currently enabled.
    ///
    /// Disabled panes cannot receive focus. When a region contains only disabled panes,
    /// the region itself cannot receive focus (will be skipped during Ctrl+Space rotation).
    ///
    /// The default implementation returns `true` (always enabled).
    fn is_enabled(&self) -> bool {
        true
    }
}
