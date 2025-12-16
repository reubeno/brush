//! Environment variables content pane.

use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::{
    layout::Alignment,
    prelude::*,
    widgets::{
        Cell, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table, TableState,
    },
};

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};

/// A content pane that displays environment variables in a scrollable table.
pub struct EnvironmentPane {
    shell: Arc<tokio::sync::Mutex<brush_core::Shell>>,
    table_state: TableState,
    scrollbar_state: ScrollbarState,
}

impl EnvironmentPane {
    /// Create a new environment pane.
    pub fn new(shell: &Arc<tokio::sync::Mutex<brush_core::Shell>>) -> Self {
        Self {
            shell: shell.clone(),
            table_state: TableState::default(),
            scrollbar_state: ScrollbarState::default(),
        }
    }
}

impl ContentPane for EnvironmentPane {
    fn name(&self) -> &'static str {
        "Environment"
    }

    fn kind(&self) -> PaneKind {
        PaneKind::Environment
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Try to get shell variables without blocking
        let variables = if let Ok(shell) = self.shell.try_lock() {
            let mut vars: Vec<(String, String)> = shell
                .env
                .iter()
                .map(|(name, var)| (name.clone(), var.value().to_cow_str(&shell).into_owned()))
                .collect();
            vars.sort_by(|a, b| a.0.cmp(&b.0));
            vars
        } else {
            // Shell is locked (command running), show loading message with modern styling
            let loading = Paragraph::new(" ⏳ Loading environment variables...")
                .style(
                    Style::default()
                        .fg(Color::Rgb(165, 243, 252)) // Cyan
                        .bg(Color::Rgb(20, 20, 30))
                        .add_modifier(Modifier::ITALIC),
                )
                .alignment(Alignment::Center);
            frame.render_widget(loading, area);
            return;
        };

        if variables.is_empty() {
            let empty = Paragraph::new(" ⚠ No environment variables ")
                .style(
                    Style::default()
                        .fg(Color::Rgb(251, 146, 60)) // Orange
                        .bg(Color::Rgb(20, 20, 30)),
                )
                .alignment(Alignment::Center);
            frame.render_widget(empty, area);
            return;
        }

        // Select first item if nothing is selected
        if self.table_state.selected().is_none() && !variables.is_empty() {
            self.table_state.select(Some(0));
        }

        // Update scrollbar state with content length
        self.scrollbar_state = self.scrollbar_state.content_length(variables.len());
        if let Some(selected) = self.table_state.selected() {
            self.scrollbar_state = self.scrollbar_state.position(selected);
        }

        // Create table with modern header styling
        let header = Row::new(vec![
            Cell::from(" 󰂆 Variable").style(
                Style::default()
                    .fg(Color::Rgb(165, 243, 252)) // Cyan
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from(" 󰂖 Value").style(
                Style::default()
                    .fg(Color::Rgb(165, 243, 252)) // Cyan
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(Style::default().bg(Color::Rgb(30, 40, 50)));

        let rows: Vec<Row<'_>> = variables
            .iter()
            .enumerate()
            .map(|(idx, (k, v))| {
                let bg = if idx % 2 == 0 {
                    Color::Rgb(20, 20, 30)
                } else {
                    Color::Rgb(25, 25, 35)
                };
                Row::new(vec![
                    Cell::from(k.as_str()).style(
                        Style::default()
                            .fg(Color::Rgb(196, 181, 253)) // Light purple
                            .bg(bg)
                            .add_modifier(Modifier::ITALIC | Modifier::BOLD),
                    ),
                    Cell::from(v.as_str())
                        .style(Style::default().fg(Color::Rgb(220, 220, 230)).bg(bg)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [Constraint::Percentage(30), Constraint::Percentage(70)],
        )
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(34, 211, 238)) // Cyan gradient highlight
                .fg(Color::Rgb(10, 10, 20))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ▶ ")
        .highlight_spacing(HighlightSpacing::Always)
        .style(
            Style::default()
                .fg(Color::Rgb(220, 220, 230))
                .bg(Color::Rgb(20, 20, 30)),
        );

        // Render table with state
        frame.render_stateful_widget(table, area, &mut self.table_state);

        // Render modern scrollbar on the right side
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::Rgb(34, 211, 238))) // Cyan
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

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
