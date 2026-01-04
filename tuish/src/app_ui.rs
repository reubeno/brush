//! Ratatui-based TUI for tuish shell.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use brush_core::{ExecutionParameters, SourceInfo};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use std::collections::HashMap;

use crate::command_input::CommandInput;
use crate::completion_pane::CompletionPane;
use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};
use crate::layout::LayoutManager;
use crate::region::{PaneId, RegionId};
use crate::region_pane_store::RegionPaneStore;
use crate::terminal_pane::TerminalPane;

/// Wrapper that allows an `Rc<RefCell<T: ContentPane>>` to be stored as `Box<dyn ContentPane>`.
/// This enables dual access: typed references for special interfaces, trait objects for generic
/// layout.
struct RcRefCellPaneWrapper<T: ContentPane> {
    inner: std::rc::Rc<std::cell::RefCell<T>>,
}

impl<T: ContentPane> RcRefCellPaneWrapper<T> {
    const fn new(inner: std::rc::Rc<std::cell::RefCell<T>>) -> Self {
        Self { inner }
    }
}

#[allow(clippy::non_send_fields_in_send_ty)]
/// SAFETY: `RcRefCellPaneWrapper` is only used in single-threaded context within tuish TUI.
/// The inner `Rc<RefCell<T>>` is never actually sent between threads.
unsafe impl<T: ContentPane + 'static> Send for RcRefCellPaneWrapper<T> {}

impl<T: ContentPane + 'static> ContentPane for RcRefCellPaneWrapper<T> {
    fn name(&self) -> &'static str {
        self.inner.borrow().name()
    }

    fn kind(&self) -> PaneKind {
        self.inner.borrow().kind()
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect) {
        self.inner.borrow_mut().render(frame, area);
    }

    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        self.inner.borrow_mut().handle_event(event)
    }

    fn on_show(&mut self) {
        self.inner.borrow_mut().on_show();
    }

    fn on_hide(&mut self) {
        self.inner.borrow_mut().on_hide();
    }

    fn border_title(&self) -> Option<String> {
        self.inner.borrow().border_title()
    }
}

// FocusedArea enum removed - now using active_region_id for unified focus model

/// Events that trigger UI updates.
#[allow(dead_code)]
pub enum UIEvent {
    /// User input event (keyboard, mouse, resize)
    Input(Event),
    /// PTY has new output to display
    PtyOutput,
    /// Shell prompt has been updated
    PromptReady,
    /// Command execution started
    CommandStarted(String),
    /// Command execution finished with exit code and control flow
    CommandFinished {
        exit_code: brush_core::ExecutionExitCode,
        should_exit: bool,
    },
    /// Periodic tick (rare, for time-based UI elements)
    Tick,
}

/// Ratatui-based TUI that displays content panes in tabs
/// and accepts command input in a bottom pane.
pub struct AppUI {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// The shell instance
    shell: Arc<Mutex<brush_core::Shell>>,

    /// Central store for all regions and panes
    store: RegionPaneStore,

    /// Direct typed references to special panes (also in store)
    /// These allow access to custom interfaces without downcasting
    primary_terminal: std::rc::Rc<std::cell::RefCell<TerminalPane>>,
    completion_pane: std::rc::Rc<std::cell::RefCell<CompletionPane>>,
    command_input_handle: std::rc::Rc<std::cell::RefCell<CommandInput>>,

    /// IDs of special panes
    command_input_pane_id: PaneId,

    /// IDs of special regions
    content_region_id: RegionId,
    command_input_region_id: RegionId,

    /// Layout manager for spatial arrangement
    layout: LayoutManager,

    /// The region that was active before completion was triggered
    pre_completion_active_region: Option<RegionId>,
    /// Navigation mode active (Ctrl+B pressed, waiting for next key)
    navigation_mode: bool,
    /// Pane marked for moving (for mark-and-move workflow)
    marked_pane_for_move: Option<PaneId>,

    /// Event channel for triggering renders
    ui_event_tx: mpsc::UnboundedSender<UIEvent>,
    ui_event_rx: mpsc::UnboundedReceiver<UIEvent>,
}

/// Result of handling a UI event.
pub enum UIEventResult {
    /// The application has been asked to terminate.
    RequestExit,
    /// The application should continue running.
    Continue,
    /// The application should execute the given command.
    ExecuteCommand(String),
    /// The application should trigger completion.
    RequestCompletion,
}

impl AppUI {
    /// Creates a new `AppUI` with the given special panes.
    ///
    /// Use `add_pane` to add general content panes after construction.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to run the UI for
    /// * `primary_terminal` - The primary terminal pane
    /// * `completion_pane` - The completion pane
    #[allow(clippy::boxed_local)]
    pub fn new(
        shell: &Arc<Mutex<brush_core::Shell>>,
        primary_terminal: Box<TerminalPane>,
        completion_pane: Box<CompletionPane>,
    ) -> Self {
        let terminal = ratatui::init();
        let mut store = RegionPaneStore::new();

        // Create event channel for UI updates
        let (ui_event_tx, ui_event_rx) = mpsc::unbounded_channel();

        // Create CommandInput
        let command_input = CommandInput::new(shell);
        let command_input_rc = std::rc::Rc::new(std::cell::RefCell::new(command_input));

        // Wrap special panes
        let primary_terminal_rc = std::rc::Rc::new(std::cell::RefCell::new(*primary_terminal));
        let completion_pane_rc = std::rc::Rc::new(std::cell::RefCell::new(*completion_pane));

        // Add panes to store
        let command_input_pane_id = store.add_pane(Box::new(RcRefCellPaneWrapper::new(
            command_input_rc.clone(),
        )) as Box<dyn ContentPane>);
        let primary_terminal_id = store.add_pane(Box::new(RcRefCellPaneWrapper::new(
            primary_terminal_rc.clone(),
        )) as Box<dyn ContentPane>);
        let _completion_pane_id = store.add_pane(Box::new(RcRefCellPaneWrapper::new(
            completion_pane_rc.clone(),
        )) as Box<dyn ContentPane>);

        // Create regions
        let content_region_id = store.create_region(
            vec![primary_terminal_id], // Start with just terminal
            true,                      // splittable
            true,                      // closeable
        );
        let command_input_region_id = store.create_region(
            vec![command_input_pane_id],
            false, // not splittable
            false, // not closeable
        );

        // Create VSplit layout: content region (80%) + command input region (20%)
        let layout = LayoutManager::new(
            crate::layout::LayoutNode::VSplit {
                id: 0, // Root split node
                top: Box::new(crate::layout::LayoutNode::Region {
                    id: 1,
                    region_id: content_region_id,
                }),
                bottom: Box::new(crate::layout::LayoutNode::Region {
                    id: 2,
                    region_id: command_input_region_id,
                }),
                split_percent: 80,
            },
            command_input_region_id, // Start with command input focused for typing
        );

        let mut app = Self {
            terminal,
            shell: shell.clone(),
            store,
            primary_terminal: primary_terminal_rc,
            completion_pane: completion_pane_rc,
            command_input_handle: command_input_rc,
            command_input_pane_id,
            content_region_id,
            command_input_region_id,
            layout,
            pre_completion_active_region: None,
            navigation_mode: false,
            marked_pane_for_move: None,
            ui_event_tx,
            ui_event_rx,
        };

        // Focus the command input pane initially
        if let Some(pane) = app.store.get_pane_mut(command_input_pane_id) {
            let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
        }

        app
    }

    /// Adds a content pane to the content region.
    ///
    /// Returns the `PaneId` assigned to the new pane.
    pub fn add_pane(&mut self, pane: Box<dyn ContentPane>) -> PaneId {
        // Add pane to store
        let pane_id = self.store.add_pane(pane);

        // Add to content region
        if let Some(region) = self.store.get_region_mut(self.content_region_id) {
            region.add_pane(pane_id);
        }

        pane_id
    }

    /// Writes output to the primary terminal pane.
    ///
    /// This allows the UI to display messages in the terminal between command executions.
    #[allow(dead_code)]
    pub fn write_to_terminal(&self, data: &[u8]) {
        self.primary_terminal.borrow_mut().process_output(data);
    }

    /// Sets the currently running command to display in the terminal pane's border.
    ///
    /// Pass `None` to clear the running command display.
    pub fn set_running_command(&self, command: Option<String>) {
        self.primary_terminal
            .borrow_mut()
            .set_running_command(command);
    }

    /// Finalizes the current command's output, storing it in history.
    ///
    /// Call this when a command finishes execution.
    pub fn finalize_command(&self, exit_code: Option<u8>) {
        self.primary_terminal
            .borrow_mut()
            .finalize_command(exit_code);
    }

    /// Draws the UI with content panes and command input.
    #[allow(clippy::too_many_lines)]
    pub fn render(&mut self) -> Result<(), std::io::Error> {
        let focused_region_id = self.layout.focused_region_id();
        let navigation_mode = self.navigation_mode;
        let command_input_pane_id = self.command_input_pane_id;
        let command_input_region_id = self.command_input_region_id;
        let command_input_handle = self.command_input_handle.clone();

        // Check if we should show the completion pane instead of normal panes
        let show_completion = self.completion_pane.borrow().is_active();

        // Borrow fields separately to avoid capturing `self` in the closure
        let completion_pane = &self.completion_pane;
        let store = &mut self.store;
        let layout = &self.layout;

        self.terminal.draw(|f| {
            // Always reserve space for hint bar at bottom
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(1)])
                .split(f.area());
            let content_area = chunks[0];
            let hint_bar_area = chunks[1];

            // Track pane rects for cursor positioning
            let mut pane_rects: HashMap<PaneId, ratatui::layout::Rect> = HashMap::new();

            // Render entire layout tree (includes CommandInput region)
            if show_completion {
                // Special case: show completion pane fullscreen
                // (completion pane renders its own borders internally)
                completion_pane.borrow_mut().render(f, content_area);
            } else {
                // Get all regions with their rendering positions
                let regions = layout.render(content_area);

                // Render each region
                for (region_id, rect) in regions {
                    let is_focused_region = Some(region_id) == focused_region_id;

                    // Get region info from store
                    let region = match store.get_region(region_id) {
                        Some(r) => r,
                        None => continue,
                    };
                    
                    let pane_ids = region.panes();
                    let focused_pane_id = region.focused_pane();

                    // Consistent theme colors
                    let bg_base = Color::Rgb(22, 22, 30);      // Main background
                    let bg_surface = Color::Rgb(30, 30, 40);   // Elevated surfaces
                    let bg_highlight = Color::Rgb(45, 45, 60); // Hover/selected
                    let text_primary = Color::Rgb(230, 230, 245);
                    let text_muted = Color::Rgb(140, 140, 160);
                    let border_dim = Color::Rgb(55, 55, 75);

                    // If region has multiple panes, render a tab bar
                    if pane_ids.len() > 1 {
                        // Split area for tabs bar + content
                        let region_chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(1), // Tab bar
                                Constraint::Min(0),    // Content
                            ])
                            .split(rect);

                        // Build tab titles
                        let tab_titles: Vec<String> = pane_ids.iter().filter_map(|&pane_id| {
                            store.get_pane(pane_id).map(|p| p.name().to_string())
                        }).collect();

                        // Accent colors for tabs (used as subtle indicators)
                        let accent_colors = [
                            Color::Rgb(139, 92, 246),  // Purple
                            Color::Rgb(34, 211, 238),  // Cyan
                            Color::Rgb(251, 146, 60),  // Orange
                            Color::Rgb(236, 72, 153),  // Pink
                            Color::Rgb(134, 239, 172), // Green
                            Color::Rgb(250, 204, 21),  // Yellow
                        ];

                        // Modern tab bar: render as styled spans directly
                        let mut tab_spans: Vec<Span<'_>> = Vec::new();
                        
                        for (i, t) in tab_titles.iter().enumerate() {
                            let accent = accent_colors[i % accent_colors.len()];
                            let pane_id = pane_ids[i];
                            let is_selected = pane_id == focused_pane_id;

                            if is_selected {
                                // Selected tab: accent color text, subtle background
                                tab_spans.push(Span::styled(
                                    format!(" ● {t} "),
                                    Style::default()
                                        .fg(accent)
                                        .bg(bg_highlight)
                                        .add_modifier(Modifier::BOLD)
                                ));
                            } else {
                                // Unselected tab: muted
                                tab_spans.push(Span::styled(
                                    format!("   {t} "),
                                    Style::default()
                                        .fg(text_muted)
                                        .bg(bg_surface)
                                ));
                            }
                        }
                        
                        // Fill remaining space with background
                        let tab_line = Line::from(tab_spans);
                        let tab_para = Paragraph::new(tab_line)
                            .style(Style::default().bg(bg_surface));
                        f.render_widget(tab_para, region_chunks[0]);

                        // Render selected pane content area
                        let selected_index = pane_ids.iter().position(|&id| id == focused_pane_id).unwrap_or(0);
                        let accent = accent_colors[selected_index % accent_colors.len()];
                        
                        // Check if this pane is marked for moving
                        let is_marked = self.marked_pane_for_move == Some(focused_pane_id);
                        
                        let border_color = if is_marked {
                            Color::Rgb(255, 215, 0) // Gold when marked
                        } else if is_focused_region {
                            accent
                        } else {
                            border_dim
                        };

                        // Get title from selected pane
                        let mut title = store.get_pane(focused_pane_id)
                            .map_or_else(
                                || "Pane".to_string(),
                                |p| p.border_title().unwrap_or_else(|| p.name().to_string())
                            );
                        
                        if is_marked {
                            title = format!("󰃀 MARKED: {title}");
                        }

                        let title_style = if is_marked {
                            Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)
                        } else if is_focused_region {
                            Style::default().fg(text_primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(text_muted)
                        };

                        // Clean, minimal border - no title icon clutter
                        let block = Block::default()
                            .borders(Borders::ALL)
                            .border_type(if is_marked { BorderType::Double } else { BorderType::Rounded })
                            .border_style(Style::default().fg(border_color))
                            .style(Style::default().bg(bg_base))
                            .title(Line::from(format!(" {title} ")).style(title_style));

                        let inner = block.inner(region_chunks[1]);
                        f.render_widget(block, region_chunks[1]);

                        // Track rect for cursor positioning
                        pane_rects.insert(focused_pane_id, inner);

                        if let Some(pane) = store.get_pane_mut(focused_pane_id) {
                            pane.render(f, inner);
                        }
                    } else {
                        // Single pane in region - render with border
                        let pane_id = focused_pane_id;
                        let accent = Color::Rgb(45, 212, 191); // Teal accent (distinct from purple terminal)

                        let border_color = if is_focused_region {
                            accent
                        } else {
                            border_dim
                        };

                        let title = store.get_pane(pane_id)
                            .map_or_else(
                                || "Pane".to_string(),
                                |p| p.border_title().unwrap_or_else(|| p.name().to_string())
                            );

                        let title_style = if is_focused_region {
                            Style::default().fg(text_primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(text_muted)
                        };

                        let block = Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(border_color))
                            .style(Style::default().bg(bg_base))
                            .title(Line::from(format!(" {title} ")).style(title_style));

                        let inner = block.inner(rect);
                        f.render_widget(block, rect);

                        // Track rect for cursor positioning
                        pane_rects.insert(pane_id, inner);

                        if let Some(pane) = store.get_pane_mut(pane_id) {
                            pane.render(f, inner);
                        }
                    }
                }
            }

            // Set cursor if CommandInput region is active
            if focused_region_id == Some(command_input_region_id) && !navigation_mode {
                if let Some(&cmd_rect) = pane_rects.get(&command_input_pane_id) {
                    if let Some((cursor_x, cursor_y)) = command_input_handle.borrow_mut()
                        .render_with_cursor(f, cmd_rect)
                    {
                        f.set_cursor_position((cursor_x, cursor_y));
                    }
                }
            }

            // Render hint bar at bottom (always visible, style changes based on mode)
            let (hint_text, bg_color, fg_color) = if navigation_mode {
                if self.marked_pane_for_move.is_some() {
                    // Gold background when a pane is marked for moving
                    (
                        " 󰃀 MARKED: Navigate to target region (n/p) then press Shift+M to MOVE pane here, or Esc to cancel ".to_string(),
                        Color::Rgb(255, 215, 0),  // Gold
                        Color::Rgb(0, 0, 0),
                    )
                } else {
                    // Yellow background for normal nav mode
                    (
                        " ⚡ NAV: t/e/h/a/f/c=panes, i=input, Tab=cycle, Ctrl+Space=region, v/s=split, m=mark, Esc=exit ".to_string(),
                        Color::Rgb(250, 204, 21),  // Yellow
                        Color::Rgb(0, 0, 0),
                    )
                }
            } else {
                // Normal mode: muted hint bar
                (
                    " Ctrl+Space: switch panes │ Ctrl+B: navigation mode ".to_string(),
                    Color::Rgb(60, 60, 70),   // Dark gray
                    Color::Rgb(160, 160, 170), // Light gray text
                )
            };

            let hint_bar = Paragraph::new(hint_text.as_str())
                .style(
                    Style::default()
                        .bg(bg_color)
                        .fg(fg_color)
                        .add_modifier(if navigation_mode { Modifier::BOLD } else { Modifier::empty() })
                )
                .alignment(Alignment::Center);
            f.render_widget(hint_bar, hint_bar_area);
        })?;

        Ok(())
    }

    fn set_focus_to_command_input(&mut self) {
        // Unfocus current pane in current region
        if let Some(current_region_id) = self.layout.focused_region_id() {
            if let Some(pane_id) = self.store.get_region_focused_pane(current_region_id) {
                if let Some(pane) = self.store.get_pane_mut(pane_id) {
                    let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                }
            }
        }

        // Focus command input region
        self.layout.set_focused_region(self.command_input_region_id);

        // Focus the pane in that region
        if let Some(pane) = self.store.get_pane_mut(self.command_input_pane_id) {
            let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
        }
    }

    /// Cycles focus to the next region, skipping regions where all panes are disabled.
    fn focus_next_region(&mut self) {
        // Unfocus current pane in current region
        if let Some(current_region_id) = self.layout.focused_region_id() {
            if let Some(pane_id) = self.store.get_region_focused_pane(current_region_id) {
                if let Some(pane) = self.store.get_pane_mut(pane_id) {
                    let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                }
            }
        }

        // Cycle through regions to find one with an enabled pane
        let regions = self.layout.get_all_region_ids();
        let start_region = self.layout.focused_region_id();

        for _ in 0..regions.len() {
            self.layout.focus_next_region();

            // Check if this region is focusable
            if let Some(new_region_id) = self.layout.focused_region_id() {
                if self.store.is_region_focusable(new_region_id) {
                    // Focus the pane in this region
                    if let Some(pane_id) = self.store.get_region_focused_pane(new_region_id) {
                        if let Some(pane) = self.store.get_pane_mut(pane_id) {
                            let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                        }
                    }
                    return;
                }
            }
        }

        // All regions have disabled panes - restore original
        if let Some(start) = start_region {
            self.layout.set_focused_region(start);
        }
    }

    /// Focuses the first pane of the given kind, switching regions if necessary.
    fn focus_pane_by_kind(&mut self, kind: &crate::content_pane::PaneKind) {
        // Unfocus current pane
        if let Some(current_region_id) = self.layout.focused_region_id() {
            if let Some(old_pane_id) = self.store.get_region_focused_pane(current_region_id) {
                if let Some(pane) = self.store.get_pane_mut(old_pane_id) {
                    let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                }
            }
        }

        // Find the first pane matching this kind
        let pane_ids: Vec<PaneId> = self.store.pane_ids().collect();
        for pane_id in pane_ids {
            let matches = if let Some(pane) = self.store.get_pane(pane_id) {
                std::mem::discriminant(&pane.kind()) == std::mem::discriminant(kind)
            } else {
                false
            };

            if matches {
                // Found a matching pane! Find which region contains it
                let region_ids: Vec<RegionId> = self.layout.get_all_region_ids();
                for region_id in region_ids {
                    let contains_pane = if let Some(region) = self.store.get_region(region_id) {
                        region.panes().contains(&pane_id)
                    } else {
                        false
                    };

                    if contains_pane {
                        // Select this pane in the region
                        if let Some(region) = self.store.get_region_mut(region_id) {
                            region.select_pane(pane_id);
                        }

                        // Focus this region
                        self.layout.set_focused_region(region_id);

                        // Focus the pane
                        if let Some(pane_mut) = self.store.get_pane_mut(pane_id) {
                            let _ = pane_mut.handle_event(crate::content_pane::PaneEvent::Focused);
                        }
                        return;
                    }
                }
            }
        }
    }

    /// Handles a single input event (keyboard, mouse, resize).
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn handle_input_event(
        &mut self,
        input_event: Event,
    ) -> Result<UIEventResult, std::io::Error> {
        // Check if completion pane is active
        let completion_active = self.completion_pane.borrow().is_active();

        match input_event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                // If completion is active, handle special keys
                KeyCode::Esc if completion_active => {
                    // Cancel completion
                    self.completion_pane.borrow_mut().clear();
                    // Restore focus to the region that was active before completion
                    if let Some(prev_region) = self.pre_completion_active_region.take() {
                        self.layout.set_focused_region(prev_region);
                    }
                }
                KeyCode::Enter if completion_active => {
                    // Accept completion
                    if let Some(completion) = self.completion_pane.borrow().selected_completion() {
                        let (insertion_index, delete_count) =
                            self.completion_pane.borrow().insertion_params();
                        // Apply to command input
                        self.command_input_handle.borrow_mut().apply_completion(
                            completion,
                            insertion_index,
                            delete_count,
                        );
                    }
                    self.completion_pane.borrow_mut().clear();
                    // Restore focus to the region that was active before completion
                    if let Some(prev_region) = self.pre_completion_active_region.take() {
                        self.layout.set_focused_region(prev_region);
                    }
                }
                // Tab/Shift-Tab for navigation when completion is active
                KeyCode::Tab
                    if completion_active && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    self.completion_pane.borrow_mut().handle_event(
                        crate::content_pane::PaneEvent::KeyPress(
                            KeyCode::Down,
                            KeyModifiers::empty(),
                        ),
                    );
                }
                KeyCode::BackTab | KeyCode::Tab
                    if completion_active && key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    self.completion_pane.borrow_mut().handle_event(
                        crate::content_pane::PaneEvent::KeyPress(
                            KeyCode::Up,
                            KeyModifiers::empty(),
                        ),
                    );
                }
                // Arrow keys for navigation
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
                    if completion_active =>
                {
                    self.completion_pane.borrow_mut().handle_event(
                        crate::content_pane::PaneEvent::KeyPress(key.code, key.modifiers),
                    );
                }
                // Allow typing to update buffer and re-trigger completion
                _ if completion_active => {
                    // First, let command input handle the key (updates buffer)
                    match self
                        .command_input_handle
                        .borrow_mut()
                        .handle_key(key.code, key.modifiers)
                    {
                        crate::command_input::CommandKeyResult::NoAction => {
                            // Buffer was updated, re-trigger completion
                            return Ok(UIEventResult::RequestCompletion);
                        }
                        crate::command_input::CommandKeyResult::CommandEntered(command) => {
                            // User pressed Enter with actual command text - cancel completion and
                            // execute
                            self.completion_pane.borrow_mut().clear();
                            if let Some(prev_region) = self.pre_completion_active_region.take() {
                                self.layout.set_focused_region(prev_region);
                            }
                            return Ok(UIEventResult::ExecuteCommand(command));
                        }
                        _ => {
                            // Cancel completion for other cases (e.g., Ctrl+D)
                            self.completion_pane.borrow_mut().clear();
                            if let Some(prev_region) = self.pre_completion_active_region.take() {
                                self.layout.set_focused_region(prev_region);
                            }
                        }
                    }
                }
                // Ctrl+B: Toggle navigation mode
                KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if self.navigation_mode {
                        // Exit navigation mode - refocus current pane
                        self.navigation_mode = false;
                        self.marked_pane_for_move = None;
                        if let Some(region_id) = self.layout.focused_region_id() {
                            if let Some(pane_id) = self.store.get_region_focused_pane(region_id) {
                                if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                    let _ =
                                        pane.handle_event(crate::content_pane::PaneEvent::Focused);
                                }
                            }
                        }
                    } else {
                        // Enter navigation mode - unfocus current pane
                        if let Some(region_id) = self.layout.focused_region_id() {
                            if let Some(pane_id) = self.store.get_region_focused_pane(region_id) {
                                if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                    let _ = pane
                                        .handle_event(crate::content_pane::PaneEvent::Unfocused);
                                }
                            }
                        }
                        self.navigation_mode = true;
                    }
                }
                // === NAVIGATION MODE: Plain letter keys (no Ctrl required) ===
                // Philosophy: Commands that navigate STAY in mode, actions that change context EXIT
                // mode

                // Pane Selection (stays in mode for chaining)
                KeyCode::Char('t') if self.navigation_mode => {
                    self.focus_pane_by_kind(&crate::content_pane::PaneKind::Terminal);
                }
                KeyCode::Char('e') if self.navigation_mode => {
                    self.focus_pane_by_kind(&crate::content_pane::PaneKind::Environment);
                }
                KeyCode::Char('h') if self.navigation_mode => {
                    self.focus_pane_by_kind(&crate::content_pane::PaneKind::History);
                }
                KeyCode::Char('a') if self.navigation_mode => {
                    self.focus_pane_by_kind(&crate::content_pane::PaneKind::Aliases);
                }
                KeyCode::Char('f') if self.navigation_mode => {
                    self.focus_pane_by_kind(&crate::content_pane::PaneKind::Functions);
                }
                KeyCode::Char('c') if self.navigation_mode => {
                    self.focus_pane_by_kind(&crate::content_pane::PaneKind::CallStack);
                }

                // Action: Go to Input (exits mode - you're done navigating, time to type)
                KeyCode::Char('i') if self.navigation_mode => {
                    self.navigation_mode = false;
                    self.marked_pane_for_move = None;
                    self.set_focus_to_command_input();
                }


                // Navigation mode: Tab to cycle tabs/panes in current region (forward)
                KeyCode::Tab
                    if self.navigation_mode && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    // Cycle panes within the current region
                    if let Some(region_id) = self.layout.focused_region_id() {
                        // Unfocus current pane
                        if let Some(old_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        // Cycle to next pane in region
                        if let Some(region) = self.store.get_region_mut(region_id) {
                            region.select_next_pane();
                        }

                        // Focus new pane
                        if let Some(new_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                }
                // Navigation mode: Shift+Tab to cycle tabs/panes in current region (backward)
                KeyCode::BackTab if self.navigation_mode => {
                    // Cycle panes within the current region
                    if let Some(region_id) = self.layout.focused_region_id() {
                        // Unfocus current pane
                        if let Some(old_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        // Cycle to previous pane in region
                        if let Some(region) = self.store.get_region_mut(region_id) {
                            region.select_prev_pane();
                        }

                        // Focus new pane
                        if let Some(new_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                }
                // Navigation mode: Ctrl+Space cycles regions (stays in nav mode)
                KeyCode::Char(' ')
                    if self.navigation_mode && key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.focus_next_region();
                }
                // Action: Split vertical (exits mode - new pane is ready to use)
                KeyCode::Char('v') if self.navigation_mode => {
                    self.navigation_mode = false;
                    self.marked_pane_for_move = None;
                    if let Some(focused_region_id) = self.layout.focused_region_id() {
                        // Check if this region can be split
                        let can_split = self
                            .store
                            .get_region(focused_region_id)
                            .map_or(false, |r| r.splittable());

                        if can_split {
                            // Get the currently focused pane to move it
                            let pane_to_move =
                                self.store.get_region_focused_pane(focused_region_id);

                            if let Some(pane_id) = pane_to_move {
                                // Unfocus the pane before moving
                                if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                    let _ = pane
                                        .handle_event(crate::content_pane::PaneEvent::Unfocused);
                                }

                                // Remove the pane from the original region
                                if let Some(region) = self.store.get_region_mut(focused_region_id) {
                                    region.remove_pane(pane_id);
                                }

                                // Create a new region with the moved pane
                                let new_region_id = self.store.create_region(
                                    vec![pane_id],
                                    true, // splittable
                                    true, // closeable
                                );

                                // Split the layout
                                if self.layout.split_vertical(new_region_id) {
                                    // Focus the moved pane
                                    if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                        let _ = pane
                                            .handle_event(crate::content_pane::PaneEvent::Focused);
                                    }
                                }
                            }
                        }
                    }
                }
                // Action: Split horizontal (exits mode - new pane is ready to use)
                KeyCode::Char('s') if self.navigation_mode => {
                    self.navigation_mode = false;
                    self.marked_pane_for_move = None;
                    if let Some(focused_region_id) = self.layout.focused_region_id() {
                        // Check if this region can be split
                        let can_split = self
                            .store
                            .get_region(focused_region_id)
                            .map_or(false, |r| r.splittable());

                        if can_split {
                            // Get the currently focused pane to move it
                            let pane_to_move =
                                self.store.get_region_focused_pane(focused_region_id);

                            if let Some(pane_id) = pane_to_move {
                                // Unfocus the pane before moving
                                if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                    let _ = pane
                                        .handle_event(crate::content_pane::PaneEvent::Unfocused);
                                }

                                // Remove the pane from the original region
                                if let Some(region) = self.store.get_region_mut(focused_region_id) {
                                    region.remove_pane(pane_id);
                                }

                                // Create a new region with the moved pane
                                let new_region_id = self.store.create_region(
                                    vec![pane_id],
                                    true, // splittable
                                    true, // closeable
                                );

                                // Split the layout
                                if self.layout.split_horizontal(new_region_id) {
                                    // Focus the moved pane
                                    if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                        let _ = pane
                                            .handle_event(crate::content_pane::PaneEvent::Focused);
                                    }
                                }
                            }
                        }
                    }
                }
                // Mark pane for moving (stays in mode)
                KeyCode::Char('m') if self.navigation_mode => {
                    if let Some(region_id) = self.layout.focused_region_id() {
                        if let Some(pane_id) = self.store.get_region_focused_pane(region_id) {
                            self.marked_pane_for_move = Some(pane_id);
                            // TODO: Visual indicator that pane is marked
                        }
                    }
                }

                // Move marked pane to current region (exits mode)
                KeyCode::Char('M') if self.navigation_mode => {
                    self.navigation_mode = false;

                    if let Some(marked_pane_id) = self.marked_pane_for_move.take() {
                        if let Some(target_region_id) = self.layout.focused_region_id() {
                            // Find which region currently contains the marked pane
                            let source_region_id =
                                self.layout.get_all_region_ids().into_iter().find(|&rid| {
                                    self.store
                                        .get_region(rid)
                                        .map_or(false, |r| r.panes().contains(&marked_pane_id))
                                });

                            if let Some(source_rid) = source_region_id {
                                // Don't move to same region
                                if source_rid != target_region_id {
                                    // Unfocus the pane before moving
                                    if let Some(pane) = self.store.get_pane_mut(marked_pane_id) {
                                        let _ = pane.handle_event(
                                            crate::content_pane::PaneEvent::Unfocused,
                                        );
                                    }

                                    // Remove from source region
                                    if let Some(source_region) =
                                        self.store.get_region_mut(source_rid)
                                    {
                                        source_region.remove_pane(marked_pane_id);
                                    }

                                    // Check if source region is now empty and remove it from layout
                                    let source_is_empty = self
                                        .store
                                        .get_region(source_rid)
                                        .map_or(true, |r| r.panes().is_empty());

                                    if source_is_empty {
                                        // Don't remove special regions (content and command input)
                                        if source_rid != self.content_region_id
                                            && source_rid != self.command_input_region_id
                                        {
                                            self.layout.remove_region(source_rid);
                                        }
                                    }

                                    // Add to target region
                                    if let Some(target_region) =
                                        self.store.get_region_mut(target_region_id)
                                    {
                                        target_region.add_pane(marked_pane_id);
                                        target_region.select_pane(marked_pane_id);
                                    }

                                    // Focus the moved pane
                                    if let Some(pane) = self.store.get_pane_mut(marked_pane_id) {
                                        let _ = pane
                                            .handle_event(crate::content_pane::PaneEvent::Focused);
                                    }
                                }
                            }
                        }
                    }
                }

                // Action: Close pane (exits mode - not yet implemented)
                KeyCode::Char('x') if self.navigation_mode => {
                    self.navigation_mode = false;
                    self.marked_pane_for_move = None;
                    // Close/unsplit not yet implemented
                }

                // Exit navigation mode without action
                KeyCode::Esc if self.navigation_mode => {
                    self.navigation_mode = false;
                    self.marked_pane_for_move = None;
                    // Send Focused event to current pane in focused region
                    if let Some(region_id) = self.layout.focused_region_id() {
                        if let Some(pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                }
                // Navigation mode: Ignore unrecognized keys (stay in mode)
                _ if self.navigation_mode => {
                    // Unknown key - just ignore it, stay in navigation mode
                }

                // Ctrl+Tab: Next pane in current region
                KeyCode::Tab
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    // Cycle panes within focused region
                    if let Some(region_id) = self.layout.focused_region_id() {
                        // Unfocus current pane
                        if let Some(old_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        // Cycle to next pane
                        if let Some(region) = self.store.get_region_mut(region_id) {
                            region.select_next_pane();
                        }

                        // Focus new pane
                        if let Some(new_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                }
                KeyCode::BackTab if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Cycle panes within focused region
                    if let Some(region_id) = self.layout.focused_region_id() {
                        // Unfocus current pane
                        if let Some(old_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        // Cycle to previous pane
                        if let Some(region) = self.store.get_region_mut(region_id) {
                            region.select_prev_pane();
                        }

                        // Focus new pane
                        if let Some(new_pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                }
                // Ctrl+Space cycles focus through panes and command input (legacy support)
                KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.focus_next_region();
                }
                // Ctrl+0: Jump to command input
                KeyCode::Char('0') if key.modifiers.contains(KeyModifiers::ALT) => {
                    self.set_focus_to_command_input();
                }
                // Ctrl+Q quits the application by returning None to signal shutdown
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(UIEventResult::RequestExit);
                }
                // Route all other keys to focused pane in focused region
                _ => {
                    if let Some(region_id) = self.layout.focused_region_id() {
                        if let Some(pane_id) = self.store.get_region_focused_pane(region_id) {
                            if let Some(pane) = self.store.get_pane_mut(pane_id) {
                                use crate::content_pane::{PaneEvent, PaneEventResult};
                                let result =
                                    pane.handle_event(PaneEvent::KeyPress(key.code, key.modifiers));
                                match result {
                                    PaneEventResult::Handled => {}
                                    PaneEventResult::NotHandled => {}
                                    PaneEventResult::RequestClose => {
                                        // Ctrl+D in command input - exit the shell
                                        return Ok(UIEventResult::RequestExit);
                                    }
                                    PaneEventResult::RequestExecute(cmd) => {
                                        return Ok(UIEventResult::ExecuteCommand(cmd));
                                    }
                                    PaneEventResult::RequestCompletion => {
                                        return Ok(UIEventResult::RequestCompletion);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            Event::Resize(_, _) => {
                // Handle terminal resize
                // TODO: Resize PTY and parser
            }
            _ => {}
        }

        Ok(UIEventResult::Continue)
    }

    /// Runs the main event loop, executing commands in the provided shell.
    ///
    /// This method blocks until the user quits (Ctrl+Q).
    ///
    /// # Errors
    /// Returns an error if rendering or event handling fails
    #[allow(clippy::unused_async, clippy::future_not_send)]
    pub async fn run(&mut self, mut pty_output_rx: mpsc::UnboundedReceiver<()>) -> Result<()> {
        let source_info = SourceInfo::default();
        let params = ExecutionParameters::default();

        let mut running_command: Option<
            tokio::task::JoinHandle<Result<brush_core::ExecutionResult, brush_core::Error>>,
        > = None;

        // Spawn task to poll for input events and forward to our channel
        let ui_event_tx = self.ui_event_tx.clone();

        let input_poll_task = tokio::task::spawn_blocking(move || {
            loop {
                // Poll for input with a short timeout
                if let Ok(true) = event::poll(Duration::from_millis(100)) {
                    if let Ok(evt) = event::read() {
                        if ui_event_tx.send(UIEvent::Input(evt)).is_err() {
                            // Channel closed, main loop exited - we should exit too
                            break;
                        }
                    }
                }
            }
        });

        // Initial render
        self.render()?;

        let exit_result = loop {
            // Wait for events using tokio::select!
            let ui_event = tokio::select! {
                // Check if running command finished
                result = async {
                    if let Some(ref mut handle) = running_command {
                        if handle.is_finished() {
                            Some(handle.await)
                        } else {
                            // Not finished, don't block
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            None
                        }
                    } else {
                        // No command running, yield indefinitely
                        std::future::pending().await
                    }
                }, if running_command.is_some() => {
                    if let Some(Ok(Ok(result))) = result {
                        // Command finished - extract exit code and check if we should exit shell
                        let should_exit = matches!(
                            result.next_control_flow,
                            brush_core::ExecutionControlFlow::ExitShell
                        );
                        Some(UIEvent::CommandFinished {
                            exit_code: result.exit_code,
                            should_exit,
                        })
                    } else {
                        // Command errored or still running
                        None
                    }
                }

                // PTY output arrived
                _ = pty_output_rx.recv() => {
                    Some(UIEvent::PtyOutput)
                }

                // UI event from input poll task or other sources
                evt = self.ui_event_rx.recv() => {
                    evt
                }
            };

            // Process the event if we got one
            let Some(evt) = ui_event else {
                continue;
            };

            // Handle the event
            match evt {
                UIEvent::CommandFinished {
                    exit_code,
                    should_exit,
                } => {
                    // Finalize the command output and store in history
                    let code: u8 = (&exit_code).into();
                    self.finalize_command(Some(code));

                    // Check if we should exit the shell
                    if should_exit {
                        break Ok(());
                    }

                    running_command = None;

                    // Re-enable command input pane and focus it
                    self.command_input_handle.borrow_mut().enable();
                    self.set_focus_to_command_input();

                    // Request prompt update
                    let _ = self.ui_event_tx.send(UIEvent::PromptReady);

                    // Render to show updated state
                    self.render()?;
                }

                UIEvent::PtyOutput => {
                    // PTY has new output, render it
                    self.render()?;
                }

                UIEvent::PromptReady => {
                    // Update prompt synchronously (we're already in async context)
                    let prompt_opt = if let Ok(mut shell_guard) = self.shell.try_lock() {
                        let p = shell_guard.compose_prompt().await.ok();
                        drop(shell_guard);
                        p
                    } else {
                        None
                    };

                    if let Some(prompt) = prompt_opt {
                        // Update the cached prompt
                        let mut parser = vt100::Parser::new(1, 1000, 0);
                        parser.process(prompt.as_bytes());
                        self.command_input_handle
                            .borrow_mut()
                            .set_cached_prompt(parser.screen().contents());
                        // Render to show updated prompt
                        self.render()?;
                    }
                }

                UIEvent::Input(input_event) => {
                    // Handle user input and render if needed
                    match self.handle_input_event(input_event)? {
                        UIEventResult::ExecuteCommand(command) => {
                            // User pressed Enter in command pane - execute the command
                            let shell = self.shell.clone();
                            let source_info = source_info.clone();
                            let params = params.clone();

                            // Show the running command in the terminal pane's border
                            self.set_running_command(Some(command.clone()));

                            running_command = Some(tokio::spawn(async move {
                                let mut shell = shell.lock().await;
                                let result = shell.run_string(command, &source_info, &params).await;
                                drop(shell);

                                result
                            }));

                            // Once it's running, disable command input pane and switch focus
                            self.command_input_handle.borrow_mut().disable();
                            self.focus_next_region();

                            // Render to show the change
                            self.render()?;
                        }
                        UIEventResult::RequestCompletion => {
                            // User pressed Tab - trigger completion
                            let mut shell = self.shell.lock().await;
                            let buffer = {
                                let cmd_input = self.command_input_handle.borrow();
                                cmd_input.buffer().to_string()
                            };
                            let cursor_pos = self.command_input_handle.borrow().cursor_pos();

                            if let Ok(completions) = shell.complete(&buffer, cursor_pos).await {
                                drop(shell); // Release lock

                                if completions.candidates.is_empty() {
                                    // No completions available
                                } else if completions.candidates.len() == 1 {
                                    // Auto-accept single completion
                                    let completion =
                                        completions.candidates.into_iter().next().unwrap();
                                    self.command_input_handle.borrow_mut().apply_completion(
                                        completion,
                                        completions.insertion_index,
                                        completions.delete_count,
                                    );
                                    // Render to show completion
                                    self.render()?;
                                } else {
                                    // Multiple completions - show pane
                                    // Store current focused region to restore later
                                    self.pre_completion_active_region =
                                        self.layout.focused_region_id();

                                    // Show completion pane
                                    self.completion_pane
                                        .borrow_mut()
                                        .set_completions(completions);
                                    // Render to show completion pane
                                    self.render()?;
                                }
                            } else {
                                drop(shell);
                                tracing::debug!("Completion failed");
                            }
                        }
                        UIEventResult::Continue => {
                            // Input handled, render to show changes
                            self.render()?;
                        }
                        UIEventResult::RequestExit => {
                            // User requested exit (Ctrl+Q)
                            break Ok(());
                        }
                    }
                }

                UIEvent::Tick | UIEvent::CommandStarted(_) => {
                    // Periodic update or other event that needs render
                    self.render()?;
                }
            }
        };

        // Close the receiver to signal the input task to exit
        self.ui_event_rx.close();

        // Wait a moment for the input polling task to notice and exit
        // It will exit within 100ms (the poll timeout)
        let _ = tokio::time::timeout(Duration::from_millis(200), input_poll_task).await;

        exit_result
    }
}

impl Drop for AppUI {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
