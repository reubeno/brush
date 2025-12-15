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
    widgets::{Block, BorderType, Borders, Paragraph, Tabs},
};
use tokio::sync::Mutex;

use std::collections::HashMap;

use crate::command_input::CommandInput;
use crate::completion_pane::CompletionPane;
use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};
use crate::layout::LayoutManager;
use crate::terminal_pane::TerminalPane;

/// Unique identifier for pane instances
pub type PaneId = usize;

/// Wrapper that allows an `Rc<RefCell<T: ContentPane>>` to be stored as `Box<dyn ContentPane>`.
/// This enables dual access: typed references for special interfaces, trait objects for generic layout.
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

/// Ratatui-based TUI that displays content panes in tabs
/// and accepts command input in a bottom pane.
pub struct AppUI {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// The shell instance
    shell: Arc<Mutex<brush_core::Shell>>,

    /// Unified pane storage - ALL panes stored here
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    /// Next available pane ID
    next_pane_id: PaneId,

    /// Direct typed references to special panes (also in panes `HashMap`)
    /// These allow access to custom interfaces without downcasting
    primary_terminal: std::rc::Rc<std::cell::RefCell<TerminalPane>>,
    completion_pane: std::rc::Rc<std::cell::RefCell<CompletionPane>>,

    /// ID of primary terminal pane (for layout references)
    #[allow(dead_code)]
    primary_terminal_id: PaneId,
    /// ID of completion pane (for layout references)
    #[allow(dead_code)]
    completion_pane_id: PaneId,

    /// Layout manager for flexible pane arrangements
    layout: LayoutManager,
    /// ID of currently active region in layout tree (unified focus model)
    active_region_id: crate::layout::LayoutId,

    /// Direct handle to `CommandInput` pane for cursor positioning
    command_input_handle: std::rc::Rc<std::cell::RefCell<CommandInput>>,
    /// ID of `CommandInput` pane
    command_input_pane_id: PaneId,

    /// The region that was active before completion was triggered
    pre_completion_active_region: Option<crate::layout::LayoutId>,
    /// Navigation mode active (Ctrl+B pressed, waiting for next key)
    navigation_mode: bool,
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

        // IDs: 0=command_input, 1=primary_terminal, 2=completion, 3+=others
        let command_input_pane_id = 0;
        let primary_terminal_id = 1;
        let completion_pane_id = 2;
        let next_pane_id = 3;

        // Create CommandInput
        let command_input = CommandInput::new(shell);
        let command_input_rc = std::rc::Rc::new(std::cell::RefCell::new(command_input));

        // Wrap special panes
        let primary_terminal_rc = std::rc::Rc::new(std::cell::RefCell::new(*primary_terminal));
        let completion_pane_rc = std::rc::Rc::new(std::cell::RefCell::new(*completion_pane));

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
            crate::layout::LayoutNode::VSplit {
                id: 0,  // Root split node
                top: Box::new(crate::layout::LayoutNode::Tabs {
                    id: 1,  // Content region
                    panes: vec![primary_terminal_id],  // Start with just terminal
                    selected: 0,
                    splittable: true,
                    closeable: true,
                }),
                bottom: Box::new(crate::layout::LayoutNode::Tabs {
                    id: 2,  // CommandInput region
                    panes: vec![command_input_pane_id],
                    selected: 0,
                    splittable: false,  // Can't split command input
                    closeable: false,   // Can't close command input
                }),
                split_percent: 80,
            }
        );

        // Start with content region active (region id = 1)
        let active_region_id = 1;

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

    /// Adds a content pane to the content region (region id=1).
    ///
    /// Returns the `PaneId` assigned to the new pane.
    pub fn add_pane(&mut self, pane: Box<dyn ContentPane>) -> PaneId {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;
        self.panes.insert(pane_id, pane);

        // Add to content region (id=1), not command input region (id=2)
        self.layout.add_pane_to_region(1, pane_id);

        pane_id
    }

    /// Gets a pane by ID.
    #[allow(dead_code)]
    fn get_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut dyn ContentPane> {
        self.panes
            .get_mut(&pane_id)
            .map(|p| &mut **p as &mut dyn ContentPane)
    }

    /// Writes output to the primary terminal pane.
    ///
    /// This allows the UI to display messages in the terminal between command executions.
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

    /// Draws the UI with content panes and command input.
    #[allow(clippy::too_many_lines)]
    pub fn render(&mut self) -> Result<(), std::io::Error> {
        let active_region_id = self.active_region_id;
        let navigation_mode = self.navigation_mode;
        let command_input_pane_id = self.command_input_pane_id;
        let command_input_handle = self.command_input_handle.clone();

        // Check if we should show the completion pane instead of normal panes
        let show_completion = self.completion_pane.borrow().is_active();

        // Borrow fields separately to avoid capturing `self` in the closure
        let completion_pane = &self.completion_pane;
        let panes = &mut self.panes;
        let layout = &self.layout;

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

            // Track pane rects for cursor positioning
            let mut pane_rects: HashMap<PaneId, ratatui::layout::Rect> = HashMap::new();

            // Render entire layout tree (includes CommandInput region)
            if show_completion {
                // Special case: show completion pane fullscreen
                // (completion pane renders its own borders internally)
                completion_pane.borrow_mut().render(f, content_area);
            } else {
                // Get all regions from layout tree
                let regions = layout.render_layout(content_area);

                // Render each region (which may contain multiple tabs)
                for (region_id, pane_ids, selected_tab, rect) in regions {
                    let is_focused_region = region_id == active_region_id;

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
                            panes.get(&pane_id).map(|p| p.name().to_string())
                        }).collect();

                        // Gradient colors for tabs
                        let gradient_colors = [
                            (Color::Rgb(139, 92, 246), "󰆍 "),  // Purple
                            (Color::Rgb(34, 211, 238), "󰘚 "),   // Cyan
                            (Color::Rgb(251, 146, 60), "󱑗 "),  // Orange
                            (Color::Rgb(236, 72, 153), "󰬪 "),   // Pink
                            (Color::Rgb(134, 239, 172), "󰊕 "),  // Green
                            (Color::Rgb(250, 204, 21), "󰜎 "),   // Yellow
                        ];

                        let tabs = Tabs::new(tab_titles.iter().enumerate().map(|(i, t)| {
                            let (color, icon) = gradient_colors[i % gradient_colors.len()];
                            let is_selected = i == selected_tab;

                            // Build tab text with underlined first character for hotkey hint
                            let mut spans = vec![Span::raw(format!(" {icon}"))];
                            if let Some(first_char) = t.chars().next() {
                                spans.push(Span::styled(
                                    first_char.to_string(),
                                    Style::default().add_modifier(Modifier::UNDERLINED)
                                ));
                                spans.push(Span::raw(t.chars().skip(1).collect::<String>()));
                            } else {
                                spans.push(Span::raw(t.clone()));
                            }
                            spans.push(Span::raw(" "));

                            let base_style = if is_selected {
                                Style::default()
                                    .fg(Color::Rgb(255, 255, 255))
                                    .bg(color)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                let dimmed_color = match color {
                                    Color::Rgb(r, g, b) => Color::Rgb(r / 3, g / 3, b / 3),
                                    _ => Color::Rgb(40, 42, 54),
                                };
                                Style::default()
                                    .fg(Color::Rgb(200, 200, 220))
                                    .bg(dimmed_color)
                            };

                            Line::from(spans).style(base_style)
                        }))
                        .select(selected_tab)
                        .style(Style::default().bg(Color::Rgb(15, 15, 25)))
                        .divider("│");

                        f.render_widget(tabs, region_chunks[0]);

                        // Render selected pane with border
                        let selected_pane_id = pane_ids[selected_tab];
                        let border_color = if is_focused_region {
                            // Bright color when this region is active
                            gradient_colors[selected_tab % gradient_colors.len()].0
                        } else {
                            // Dimmer color when region is not active
                            let (r, g, b) = match gradient_colors[selected_tab % gradient_colors.len()].0 {
                                Color::Rgb(r, g, b) => (r / 2, g / 2, b / 2),
                                _ => (60, 60, 80),
                            };
                            Color::Rgb(r, g, b)
                        };

                        // Get title from selected pane (shows running command for Terminal)
                        let title = panes.get(&selected_pane_id)
                            .map_or_else(
                                || "Pane".to_string(),
                                |p| p.border_title().unwrap_or_else(|| p.name().to_string())
                            );

                        let title_color = if is_focused_region {
                            Color::Rgb(220, 208, 255) // Bright when focused
                        } else {
                            Color::Rgb(150, 150, 170) // Dimmer when not focused
                        };

                        let block = Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(border_color))
                            .title(Line::from(format!(" 󰐊 {title} ")).style(
                                Style::default()
                                    .fg(title_color)
                                    .add_modifier(Modifier::BOLD)
                            ));

                        let inner = block.inner(region_chunks[1]);
                        f.render_widget(block, region_chunks[1]);

                        // Track rect for cursor positioning
                        pane_rects.insert(selected_pane_id, inner);

                        if let Some(pane) = panes.get_mut(&selected_pane_id) {
                            pane.render(f, inner);
                        }
                    } else if selected_tab < pane_ids.len() {
                        // Single pane in region - render with border
                        let pane_id = pane_ids[selected_tab];

                        let border_color = if is_focused_region {
                            Color::Rgb(139, 92, 246) // Bright purple when focused
                        } else {
                            Color::Rgb(69, 46, 123) // Dimmer purple when not focused
                        };

                        let title = panes.get(&pane_id)
                            .map_or_else(
                                || "Pane".to_string(),
                                |p| p.border_title().unwrap_or_else(|| p.name().to_string())
                            );

                        let title_color = if is_focused_region {
                            Color::Rgb(220, 208, 255) // Bright when focused
                        } else {
                            Color::Rgb(150, 150, 170) // Dimmer when not focused
                        };

                        let block = Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(border_color))
                            .title(Line::from(format!(" 󰐊 {title} ")).style(
                                Style::default()
                                    .fg(title_color)
                                    .add_modifier(Modifier::BOLD)
                            ));

                        let inner = block.inner(rect);
                        f.render_widget(block, rect);

                        // Track rect for cursor positioning
                        pane_rects.insert(pane_id, inner);

                        if let Some(pane) = panes.get_mut(&pane_id) {
                            pane.render(f, inner);
                        }
                    }
                }
            }

            // Set cursor if CommandInput is active
            if active_region_id == 2 && !navigation_mode {
                if let Some(&cmd_rect) = pane_rects.get(&command_input_pane_id) {
                    if let Some((cursor_x, cursor_y)) = command_input_handle.borrow_mut()
                        .render_with_cursor(f, cmd_rect)
                    {
                        f.set_cursor_position((cursor_x, cursor_y));
                    }
                }
            }

            // Render navigation banner at bottom if in navigation mode
            if let Some(nav_rect) = nav_area {
                // Get number of regions to conditionally show n/p
                let num_regions = layout.get_all_regions().len();
                let region_nav = if num_regions > 1 {
                    ", n/p=region"
                } else {
                    ""
                };
                
                let nav_text = format!(
                    " ⚡ NAV: Ctrl+E/H/A/F/C/T=panes, i=input, Tab=cycle{region_nav}, v/h=split, Ctrl+Space=toggle, Esc=exit "
                );
                
                let nav_indicator = Paragraph::new(nav_text)
                    .style(
                        Style::default()
                            .bg(Color::Rgb(250, 204, 21)) // Bright yellow
                            .fg(Color::Rgb(0, 0, 0))      // Black text
                            .add_modifier(Modifier::BOLD)
                    )
                    .alignment(Alignment::Center);
                f.render_widget(nav_indicator, nav_rect);
            }
        })?;

        Ok(())
    }

    fn set_focus_to_command_input(&mut self) {
        // Unfocus current pane
        if let Some(pane_id) = self.layout.focused_pane() {
            if let Some(pane) = self.panes.get_mut(&pane_id) {
                let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
            }
        }
        
        // Focus command input region (id = 2)
        self.layout.set_focused_node(2);  // CommandInput region ID
        self.active_region_id = 2;
        
        // Focus the pane
        if let Some(pane) = self.panes.get_mut(&self.command_input_pane_id) {
            let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
        }
    }

    fn set_focus_to_next_pane_or_area(&mut self) {
        // Unfocus current pane
        if let Some(old_pane_id) = self.layout.focused_pane() {
            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
            }
        }

        // Try to find the next enabled region
        let regions = self.layout.get_all_regions();
        let start_region = self.active_region_id;
        
        for _ in 0..regions.len() {
            self.layout.focus_next_region();
            self.active_region_id = self.layout.focused_node_id().unwrap_or(self.active_region_id);
            
            // Check if this region has an enabled pane
            if let Some(pane_id) = self.layout.focused_pane() {
                if let Some(pane) = self.panes.get(&pane_id) {
                    if pane.is_enabled() {
                        // Focus this pane
                        if let Some(pane_mut) = self.panes.get_mut(&pane_id) {
                            let _ = pane_mut.handle_event(crate::content_pane::PaneEvent::Focused);
                        }
                        return;
                    }
                }
            }
        }
        
        // All regions disabled - restore original focus
        self.active_region_id = start_region;
    }

    /// Focuses the first pane of the given kind, switching regions if necessary.
    fn focus_pane_by_kind(&mut self, kind: &crate::content_pane::PaneKind) {
        // Send Unfocused to currently focused pane
        let old_pane_id = self.layout.focused_pane();

        // Find the first pane matching this kind
        for (&pane_id, pane) in &self.panes {
            if std::mem::discriminant(&pane.kind()) == std::mem::discriminant(kind) {
                // Found a matching pane! Try to focus it
                if self.layout.focus_pane(pane_id) {
                    // Send Unfocused to old pane if it changed
                    if let Some(old_id) = old_pane_id {
                        if old_id != pane_id {
                            if let Some(old_pane) = self.panes.get_mut(&old_id) {
                                let _ = old_pane
                                    .handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }
                    }

                    // Send Focused to new pane
                    if let Some(new_pane) = self.panes.get_mut(&pane_id) {
                        let _ = new_pane.handle_event(crate::content_pane::PaneEvent::Focused);
                    }

                    // Update our tracked active region
                    self.active_region_id = self.layout.focused_node_id().unwrap_or(0);
                    return;
                }
            }
        }
    }

    /// Handles input events.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn handle_events(&mut self) -> Result<UIEventResult, std::io::Error> {
        // Check if completion pane is active
        let completion_active = self.completion_pane.borrow().is_active();

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    // If completion is active, handle special keys
                    KeyCode::Esc if completion_active => {
                        // Cancel completion
                        self.completion_pane.borrow_mut().clear();
                        // Restore active region
                        if let Some(prev_region) = self.pre_completion_active_region.take() {
                            self.active_region_id = prev_region;
                        }
                    }
                    KeyCode::Enter if completion_active => {
                        // Accept completion
                        if let Some(completion) =
                            self.completion_pane.borrow().selected_completion()
                        {
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
                        // Restore active region
                        if let Some(prev_region) = self.pre_completion_active_region.take() {
                            self.active_region_id = prev_region;
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
                        match self.command_input_handle.borrow_mut().handle_key(key.code, key.modifiers) {
                            crate::command_input::CommandKeyResult::NoAction => {
                                // Buffer was updated, re-trigger completion
                                return Ok(UIEventResult::RequestCompletion);
                            }
                            crate::command_input::CommandKeyResult::CommandEntered(command) => {
                                // User pressed Enter with actual command text - cancel completion and execute
                                self.completion_pane.borrow_mut().clear();
                                if let Some(prev_region) = self.pre_completion_active_region.take() {
                                    self.active_region_id = prev_region;
                                }
                                return Ok(UIEventResult::ExecuteCommand(command));
                            }
                            _ => {
                                // Cancel completion for other cases (e.g., Ctrl+D)
                                self.completion_pane.borrow_mut().clear();
                                if let Some(prev_region) = self.pre_completion_active_region.take() {
                                    self.active_region_id = prev_region;
                                }
                            }
                        }
                    }
                    // Ctrl+B: Enter navigation mode
                    KeyCode::Char('b')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && !self.navigation_mode =>
                    {
                        // Unfocus current pane when entering navigation mode
                        if let Some(pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }
                        self.navigation_mode = true;
                    }
                    // Navigation mode active: handle navigation keys (mode stays active)
                    // Navigation mode: Letter keys jump to specific pane types
                    KeyCode::Char('e')
                        if self.navigation_mode
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.focus_pane_by_kind(&crate::content_pane::PaneKind::Environment);
                    }
                    KeyCode::Char('h')
                        if self.navigation_mode
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.focus_pane_by_kind(&crate::content_pane::PaneKind::History);
                    }
                    KeyCode::Char('a')
                        if self.navigation_mode
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.focus_pane_by_kind(&crate::content_pane::PaneKind::Aliases);
                    }
                    KeyCode::Char('f')
                        if self.navigation_mode
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.focus_pane_by_kind(&crate::content_pane::PaneKind::Functions);
                    }
                    KeyCode::Char('c')
                        if self.navigation_mode
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.focus_pane_by_kind(&crate::content_pane::PaneKind::CallStack);
                    }
                    KeyCode::Char('t')
                        if self.navigation_mode
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.focus_pane_by_kind(&crate::content_pane::PaneKind::Terminal);
                    }
                    // Navigation mode: 'i' for command input (like vim insert mode)
                    KeyCode::Char('i') if self.navigation_mode => {
                        // Exit navigation mode and focus command input directly
                        self.navigation_mode = false;
                        self.set_focus_to_command_input();
                    }
                    // Navigation mode: 'n' for next region
                    KeyCode::Char('n') if self.navigation_mode => {
                        // Send Unfocused to current pane
                        if let Some(old_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        self.layout.focus_next_region();
                        self.active_region_id = self.layout.focused_node_id().unwrap_or(0);

                        // Send Focused to new pane
                        if let Some(new_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    // Navigation mode: 'p' for previous region
                    KeyCode::Char('p') if self.navigation_mode => {
                        // Send Unfocused to current pane
                        if let Some(old_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        self.layout.focus_prev_region();
                        self.active_region_id = self.layout.focused_node_id().unwrap_or(0);

                        // Send Focused to new pane
                        if let Some(new_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    // Navigation mode: Tab to cycle tabs in current region (forward)
                    KeyCode::Tab
                        if self.navigation_mode && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                    {
                        // Send Unfocused to current pane
                        if let Some(old_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        self.layout.cycle_tabs_in_focused_region(true);

                        // Send Focused to new pane
                        if let Some(new_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    // Navigation mode: Shift+Tab to cycle tabs in current region (backward)
                    KeyCode::BackTab if self.navigation_mode => {
                        // Send Unfocused to current pane
                        if let Some(old_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        self.layout.cycle_tabs_in_focused_region(false);

                        // Send Focused to new pane
                        if let Some(new_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    // Navigation mode: Ctrl+Space exits nav mode and cycles regions
                    KeyCode::Char(' ')
                        if self.navigation_mode && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.navigation_mode = false;
                        self.set_focus_to_next_pane_or_area();
                    }
                    // Navigation mode: 'v' for vertical split (side by side)
                    KeyCode::Char('v') if self.navigation_mode => {
                        // Check if current region is splittable (not CommandInput region)
                        // CommandInput region (id=2) has splittable=false
                        if self.active_region_id != 2 {
                            // Create a new Environment pane for the new region
                            let new_pane_id = self.next_pane_id;
                            self.next_pane_id += 1;
                            self.panes.insert(
                                new_pane_id,
                                Box::new(crate::environment_pane::EnvironmentPane::new(&self.shell)),
                            );

                            // Split the focused region vertically
                            if self.layout.split_vertical(new_pane_id) {
                                // Update active region ID
                                self.active_region_id = self.layout.focused_node_id().unwrap_or(0);
                                
                                // Send Unfocused to old pane
                                if let Some(old_pane_id) = self.layout.focused_pane() {
                                    if old_pane_id != new_pane_id {
                                        if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                            let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                                        }
                                    }
                                }
                                
                                // Send Focused to new pane
                                if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                    let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                                }
                            }
                        }
                    }
                    // Navigation mode: 'h' for horizontal split (top and bottom)
                    KeyCode::Char('h') if self.navigation_mode => {
                        // Check if current region is splittable (not CommandInput region)
                        // CommandInput region (id=2) has splittable=false
                        if self.active_region_id != 2 {
                            // Create a new Environment pane for the new region
                            let new_pane_id = self.next_pane_id;
                            self.next_pane_id += 1;
                            self.panes.insert(
                                new_pane_id,
                                Box::new(crate::environment_pane::EnvironmentPane::new(&self.shell)),
                            );

                            // Split the focused region horizontally
                            if self.layout.split_horizontal(new_pane_id) {
                                // Update active region ID
                                self.active_region_id = self.layout.focused_node_id().unwrap_or(0);
                                
                                // Send Unfocused to old pane
                                if let Some(old_pane_id) = self.layout.focused_pane() {
                                    if old_pane_id != new_pane_id {
                                        if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                            let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                                        }
                                    }
                                }
                                
                                // Send Focused to new pane
                                if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                    let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                                }
                            }
                        }
                    }
                    // Navigation mode: 'x' for close - not yet implemented
                    KeyCode::Char('x') if self.navigation_mode => {
                        // Close/unsplit not yet implemented - silently ignore
                    }
                    // Navigation mode: Esc to exit (only way out)
                    KeyCode::Esc if self.navigation_mode => {
                        self.navigation_mode = false;
                        // Send Focused event to current pane
                        if let Some(pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    // Navigation mode: Ignore unrecognized keys (stay in mode)
                    _ if self.navigation_mode => {
                        // Unknown key - just ignore it, stay in navigation mode
                    }

                    // Ctrl+Tab: Next tab in current region
                    KeyCode::Tab
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                    {
                        // Send Unfocused to current pane
                        if let Some(old_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        self.layout.cycle_tabs_in_focused_region(true);

                        // Send Focused to new pane
                        if let Some(new_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    KeyCode::BackTab if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Send Unfocused to current pane
                        if let Some(old_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&old_pane_id) {
                                let _ =
                                    pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
                            }
                        }

                        self.layout.cycle_tabs_in_focused_region(false);

                        // Send Focused to new pane
                        if let Some(new_pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&new_pane_id) {
                                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
                            }
                        }
                    }
                    // Ctrl+Space cycles focus through panes and command input (legacy support)
                    KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.set_focus_to_next_pane_or_area();
                    }
                    // Ctrl+0: Jump to command input
                    KeyCode::Char('0') if key.modifiers.contains(KeyModifiers::ALT) => {
                        self.set_focus_to_command_input();
                    }
                    // Ctrl+Q quits the application by returning None to signal shutdown
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(UIEventResult::RequestExit);
                    }
                    // Route all other keys to active pane
                    _ => {
                        if let Some(pane_id) = self.layout.focused_pane() {
                            if let Some(pane) = self.panes.get_mut(&pane_id) {
                                use crate::content_pane::{PaneEvent, PaneEventResult};
                                let result =
                                    pane.handle_event(PaneEvent::KeyPress(key.code, key.modifiers));
                                match result {
                                    PaneEventResult::Handled => {}
                                    PaneEventResult::NotHandled => {}
                                    PaneEventResult::RequestClose => {}
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
                },
                Event::Resize(_, _) => {
                    // Handle terminal resize
                    // TODO: Resize PTY and parser
                }
                _ => {}
            }
        }

        Ok(UIEventResult::Continue)
    }

    /// Runs the main event loop, executing commands in the provided shell.
    ///
    /// This method blocks until the user quits (Ctrl+Q).
    ///
    /// # Errors
    /// Returns an error if rendering or event handling fails
    #[allow(clippy::unused_async, clippy::future_not_send, clippy::await_holding_refcell_ref)]
    pub async fn run(&mut self) -> Result<()> {
        let source_info = SourceInfo::default();
        let params = ExecutionParameters::default();

        let mut running_command: Option<
            tokio::task::JoinHandle<Result<brush_core::ExecutionResult, brush_core::Error>>,
        > = None;

        loop {
            // See if we're waiting on a command (and it finished).
            if let Some(handle) = &mut running_command {
                if handle.is_finished() {
                    if let Ok(result) = handle.await? {
                        // Write a status message to the terminal pane
                        let exit_code: u8 = (&result.exit_code).into();
                        let status_msg = std::format!(
                            "\r\n----------- [tuish: command exited with code {exit_code}] ----------- \r\n\r\n"
                        );
                        self.write_to_terminal(status_msg.as_bytes());

                        if matches!(
                            result.next_control_flow,
                            brush_core::ExecutionControlFlow::ExitShell
                        ) {
                            break;
                        }
                    }

                    running_command = None;

                    // Clear the running command display
                    self.set_running_command(None);

                    // Re-enable command input pane and focus it
                    self.command_input_handle.borrow_mut().enable();
                    self.set_focus_to_command_input();
                }
            }

            // Update the command input area if it's not busy.
            // SAFETY: We hold the RefCell borrow across an await point, but this is safe because:
            // 1. This is single-threaded code (tokio::task::LocalSet)
            // 2. No other code path borrows command_input_handle during try_refresh()
            // 3. The async operation doesn't yield to other tasks that could borrow it
            #[allow(clippy::await_holding_refcell_ref)]
            {
                self.command_input_handle.borrow_mut().try_refresh().await;
            }

            // Render the UI
            self.render()?;

            // Handle input events.
            match self.handle_events()? {
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
                    self.set_focus_to_next_pane_or_area();

                    // TODO: Check for exit signal from command execution
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
                            let completion = completions.candidates.into_iter().next().unwrap();
                            self.command_input_handle.borrow_mut().apply_completion(
                                completion,
                                completions.insertion_index,
                                completions.delete_count,
                            );
                        } else {
                            // Multiple completions - show pane
                            // Store current active region to restore later
                            self.pre_completion_active_region = Some(self.active_region_id);

                            // Show completion pane
                            self.completion_pane
                                .borrow_mut()
                                .set_completions(completions);
                        }
                    } else {
                        drop(shell);
                        tracing::debug!("Completion failed");
                    }
                }
                UIEventResult::Continue => {}
                UIEventResult::RequestExit => {
                    // User requested exit (Ctrl+Q)
                    break;
                }
            }
        }

        Ok(())
    }
}

impl Drop for AppUI {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
