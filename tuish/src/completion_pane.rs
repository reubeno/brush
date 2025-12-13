//! Completion pane for tuish.
//!
//! Displays completion candidates in a scrollable list with rich metadata.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use tokio::sync::Mutex;

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};

/// Kind of completion candidate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum CompletionKind {
    /// A file
    File,
    /// A directory
    Directory,
    /// A command/builtin
    Command,
    /// A variable
    Variable,
    /// Unknown/other
    Unknown,
}

/// A completion candidate with metadata
struct CompletionCandidate {
    /// The completion value to insert
    value: String,
    /// Optional description
    description: Option<String>,
    /// Kind of completion
    kind: CompletionKind,
}

/// Completion pane state
pub struct CompletionPane {
    /// Reference to the shell
    _shell: Arc<Mutex<brush_core::Shell>>,
    /// Completion candidates
    candidates: Vec<CompletionCandidate>,
    /// Currently selected candidate index
    selected_index: usize,
    /// Scroll offset for the list
    scroll_offset: usize,
    /// Where to insert the completion in the buffer
    insertion_index: usize,
    /// How many characters to delete when inserting
    delete_count: usize,
    /// Whether this pane is currently active
    is_active: bool,
}

impl CompletionPane {
    /// Creates a new completion pane.
    pub fn new(shell: &Arc<Mutex<brush_core::Shell>>) -> Self {
        Self {
            _shell: shell.clone(),
            candidates: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            insertion_index: 0,
            delete_count: 0,
            is_active: false,
        }
    }

    /// Sets the completion candidates to display.
    pub fn set_completions(&mut self, completions: brush_core::completion::Completions) {
        self.candidates = completions
            .candidates
            .into_iter()
            .map(|value| {
                // Infer the kind from the value
                let kind = if value.ends_with('/') {
                    CompletionKind::Directory
                } else if value.starts_with('$') {
                    CompletionKind::Variable
                } else {
                    // Could be file or command, default to unknown for now
                    CompletionKind::Unknown
                };

                CompletionCandidate {
                    value,
                    description: None,
                    kind,
                }
            })
            .collect();

        self.insertion_index = completions.insertion_index;
        self.delete_count = completions.delete_count;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.is_active = true;
    }

    /// Returns the currently selected completion value, if any.
    pub fn selected_completion(&self) -> Option<String> {
        self.candidates
            .get(self.selected_index)
            .map(|c| c.value.clone())
    }

    /// Returns the insertion parameters for applying the completion.
    pub fn insertion_params(&self) -> (usize, usize) {
        (self.insertion_index, self.delete_count)
    }

    /// Clears the completion state.
    pub fn clear(&mut self) {
        self.candidates.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.is_active = false;
    }

    /// Checks if the pane has active completions.
    pub fn is_active(&self) -> bool {
        self.is_active && !self.candidates.is_empty()
    }

    /// Moves selection down.
    fn select_next(&mut self) {
        if self.selected_index + 1 < self.candidates.len() {
            self.selected_index += 1;
            self.adjust_scroll();
        }
    }

    /// Moves selection up.
    fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }

    /// Adjusts scroll offset to keep selected item visible.
    fn adjust_scroll(&mut self) {
        // This will be adjusted based on visible height during render
        // For now, just ensure selected is in reasonable range
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
    }

    /// Gets the style for a completion kind.
    fn kind_style(kind: CompletionKind) -> Style {
        match kind {
            CompletionKind::Directory => Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            CompletionKind::File => Style::default().fg(Color::White),
            CompletionKind::Command => Style::default().fg(Color::Cyan),
            CompletionKind::Variable => Style::default().fg(Color::Yellow),
            CompletionKind::Unknown => Style::default().fg(Color::Gray),
        }
    }

    /// Gets a symbol for a completion kind.
    fn kind_symbol(kind: CompletionKind) -> &'static str {
        match kind {
            CompletionKind::Directory => "📁",
            CompletionKind::File => "📄",
            CompletionKind::Command => "⚙️ ",
            CompletionKind::Variable => "$",
            CompletionKind::Unknown => " ",
        }
    }
}

impl ContentPane for CompletionPane {
    fn name(&self) -> &'static str {
        "Completions"
    }

    fn kind(&self) -> PaneKind {
        // We don't have a specific kind for completion panes yet
        // Could add PaneKind::Completion later
        PaneKind::Terminal
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if self.candidates.is_empty() {
            let empty_msg = Paragraph::new("No completions available")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(empty_msg, area);
            return;
        }

        // Split the area: list (top 70%) and details (bottom 30%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        // Adjust scroll to keep selected item visible
        let visible_height = chunks[0].height.saturating_sub(2) as usize; // Account for borders
        if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index.saturating_sub(visible_height - 1);
        } else if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }

        // Render the candidate list
        let items: Vec<ListItem<'_>> = self
            .candidates
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|(i, candidate)| {
                let is_selected = i == self.selected_index;
                let symbol = Self::kind_symbol(candidate.kind);
                let kind_style = Self::kind_style(candidate.kind);

                let content = Line::from(vec![
                    Span::raw(symbol),
                    Span::raw(" "),
                    Span::styled(&candidate.value, kind_style),
                ]);

                let style = if is_selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };

                ListItem::new(content).style(style)
            })
            .collect();

        let total = self.candidates.len();
        let title = format!(
            " Completions ({}/{}) - ↑↓ to navigate, Enter to accept, Esc to cancel ",
            self.selected_index + 1,
            total
        );

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        );

        frame.render_widget(list, chunks[0]);

        // Render details for selected candidate
        if let Some(candidate) = self.candidates.get(self.selected_index) {
            let mut detail_lines = vec![
                Line::from(vec![
                    Span::styled(
                        "Value: ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&candidate.value),
                ]),
                Line::from(vec![
                    Span::styled(
                        "Type: ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:?}", candidate.kind),
                        Self::kind_style(candidate.kind),
                    ),
                ]),
            ];

            if let Some(desc) = &candidate.description {
                detail_lines.push(Line::from(vec![Span::styled(
                    "Description: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )]));
                detail_lines.push(Line::from(desc.as_str()));
            }

            let details = Paragraph::new(detail_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Details ")
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .wrap(Wrap { trim: false });

            frame.render_widget(details, chunks[1]);
        }
    }

    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        match event {
            PaneEvent::KeyPress(code, modifiers) => {
                match code {
                    KeyCode::Down | KeyCode::Char('j')
                        if !modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.select_next();
                        PaneEventResult::Handled
                    }
                    KeyCode::Up | KeyCode::Char('k')
                        if !modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.select_previous();
                        PaneEventResult::Handled
                    }
                    KeyCode::PageDown => {
                        // Jump down by 10
                        for _ in 0..10 {
                            self.select_next();
                        }
                        PaneEventResult::Handled
                    }
                    KeyCode::PageUp => {
                        // Jump up by 10
                        for _ in 0..10 {
                            self.select_previous();
                        }
                        PaneEventResult::Handled
                    }
                    KeyCode::Home => {
                        self.selected_index = 0;
                        self.scroll_offset = 0;
                        PaneEventResult::Handled
                    }
                    KeyCode::End => {
                        self.selected_index = self.candidates.len().saturating_sub(1);
                        self.adjust_scroll();
                        PaneEventResult::Handled
                    }
                    KeyCode::Enter | KeyCode::Tab => {
                        // Signal acceptance (handled by AppUI)
                        PaneEventResult::Handled
                    }
                    KeyCode::Esc => {
                        // Signal cancellation (handled by AppUI)
                        PaneEventResult::Handled
                    }
                    _ => PaneEventResult::NotHandled,
                }
            }
            PaneEvent::Focused => {
                // Pane gained focus
                PaneEventResult::Handled
            }
            PaneEvent::Unfocused => {
                // Pane lost focus
                PaneEventResult::Handled
            }
            PaneEvent::Resized { .. } => PaneEventResult::Handled,
        }
    }

    fn border_title(&self) -> Option<String> {
        Some(format!("Showing {} completions", self.candidates.len()))
    }
}
