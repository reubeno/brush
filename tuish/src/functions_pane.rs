//! Shell functions content pane.

use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::{
    prelude::*,
    widgets::{
        Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
        TableState,
    },
};

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};

/// A content pane that displays shell function definitions in a scrollable table.
pub struct FunctionsPane {
    shell: Arc<tokio::sync::Mutex<brush_core::Shell>>,
    table_state: TableState,
    scrollbar_state: ScrollbarState,
}

impl FunctionsPane {
    /// Create a new functions pane.
    pub fn new(shell: &Arc<tokio::sync::Mutex<brush_core::Shell>>) -> Self {
        Self {
            shell: shell.clone(),
            table_state: TableState::default(),
            scrollbar_state: ScrollbarState::default(),
        }
    }
}

impl ContentPane for FunctionsPane {
    fn name(&self) -> &'static str {
        "Functions"
    }

    fn kind(&self) -> PaneKind {
        PaneKind::Functions
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Try to get shell functions without blocking
        let functions = if let Ok(shell) = self.shell.try_lock() {
            let mut functions: Vec<(String, String)> = shell
                .funcs()
                .iter()
                .map(|(name, reg)| {
                    // Get the function definition as a string
                    let def_str = reg.definition().to_string();
                    (name.clone(), def_str)
                })
                .collect();
            functions.sort_by(|a, b| a.0.cmp(&b.0));
            functions
        } else {
            // Shell is locked (command running), show loading message
            let loading = ratatui::widgets::Paragraph::new("⏳ Loading functions...")
                .style(Style::default().fg(Color::White));
            frame.render_widget(loading, area);
            return;
        };

        if functions.is_empty() {
            let empty = ratatui::widgets::Paragraph::new("⚠ No functions defined")
                .style(Style::default().fg(Color::White));
            frame.render_widget(empty, area);
            return;
        }

        // Select first item if nothing is selected
        if self.table_state.selected().is_none() && !functions.is_empty() {
            self.table_state.select(Some(0));
        }

        // Update scrollbar state with content length
        self.scrollbar_state = self.scrollbar_state.content_length(functions.len());
        if let Some(selected) = self.table_state.selected() {
            self.scrollbar_state = self.scrollbar_state.position(selected);
        }

        // Create table with header and rows
        let header = Row::new(vec![
            Cell::from("Function").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Definition").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().bg(Color::DarkGray));

        let rows: Vec<Row<'_>> = functions
            .iter()
            .map(|(name, definition)| {
                // Truncate the definition for display in table (first line only)
                let first_line = definition
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(60)
                    .collect::<String>();
                let display_def = if definition.lines().count() > 1
                    || definition.lines().next().is_some_and(|l| l.len() > 60)
                {
                    format!("{}...", first_line)
                } else {
                    first_line
                };

                Row::new(vec![
                    Cell::from(name.as_str()).style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Cell::from(display_def).style(Style::default().fg(Color::Cyan)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [Constraint::Percentage(25), Constraint::Percentage(75)],
        )
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always)
        .style(Style::default().fg(Color::White));

        // Render table with state
        frame.render_stateful_widget(table, area, &mut self.table_state);

        // Render scrollbar on the right side
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let scrollbar_area = area.inner(Margin {
            vertical: 1, // Leave space for header
            horizontal: 0,
        });

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut self.scrollbar_state);
    }

    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        match event {
            PaneEvent::KeyPress(key, _modifiers) => match key {
                KeyCode::Up => {
                    self.table_state.select_previous();
                    if let Some(selected) = self.table_state.selected() {
                        self.scrollbar_state = self.scrollbar_state.position(selected);
                    }
                    PaneEventResult::Handled
                }
                KeyCode::Down => {
                    self.table_state.select_next();
                    if let Some(selected) = self.table_state.selected() {
                        self.scrollbar_state = self.scrollbar_state.position(selected);
                    }
                    PaneEventResult::Handled
                }
                KeyCode::PageUp => {
                    // Move up by 10 rows
                    for _ in 0..10 {
                        self.table_state.select_previous();
                    }
                    if let Some(selected) = self.table_state.selected() {
                        self.scrollbar_state = self.scrollbar_state.position(selected);
                    }
                    PaneEventResult::Handled
                }
                KeyCode::PageDown => {
                    // Move down by 10 rows
                    for _ in 0..10 {
                        self.table_state.select_next();
                    }
                    if let Some(selected) = self.table_state.selected() {
                        self.scrollbar_state = self.scrollbar_state.position(selected);
                    }
                    PaneEventResult::Handled
                }
                KeyCode::Home => {
                    self.table_state.select_first();
                    self.scrollbar_state = self.scrollbar_state.position(0);
                    PaneEventResult::Handled
                }
                KeyCode::End => {
                    self.table_state.select_last();
                    if let Some(selected) = self.table_state.selected() {
                        self.scrollbar_state = self.scrollbar_state.position(selected);
                    }
                    PaneEventResult::Handled
                }
                _ => PaneEventResult::NotHandled,
            },
            _ => PaneEventResult::NotHandled,
        }
    }
}
