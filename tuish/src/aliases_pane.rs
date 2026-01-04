//! Shell aliases content pane.

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

/// A content pane that displays shell aliases in a scrollable table.
pub struct AliasesPane {
    shell: Arc<tokio::sync::Mutex<brush_core::Shell>>,
    table_state: TableState,
    scrollbar_state: ScrollbarState,
}

impl AliasesPane {
    /// Create a new aliases pane.
    pub fn new(shell: &Arc<tokio::sync::Mutex<brush_core::Shell>>) -> Self {
        Self {
            shell: shell.clone(),
            table_state: TableState::default(),
            scrollbar_state: ScrollbarState::default(),
        }
    }
}

impl ContentPane for AliasesPane {
    fn name(&self) -> &'static str {
        "Aliases"
    }

    fn kind(&self) -> PaneKind {
        PaneKind::Aliases
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Try to get shell aliases without blocking
        let aliases = if let Ok(shell) = self.shell.try_lock() {
            let mut aliases: Vec<(String, String)> = shell
                .aliases()
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            aliases.sort_by(|a, b| a.0.cmp(&b.0));
            aliases
        } else {
            // Shell is locked (command running), show modern loading message
            let loading = Paragraph::new(" ⏳ Loading aliases...")
                .style(
                    Style::default()
                        .fg(Color::Rgb(236, 72, 153)) // Pink
                        .bg(Color::Rgb(20, 20, 30))
                        .add_modifier(Modifier::ITALIC),
                )
                .alignment(Alignment::Center);
            frame.render_widget(loading, area);
            return;
        };

        if aliases.is_empty() {
            let empty = Paragraph::new(" ⚠ No aliases defined ")
                .style(
                    Style::default()
                        .fg(Color::Rgb(236, 72, 153)) // Pink
                        .bg(Color::Rgb(20, 20, 30)),
                )
                .alignment(Alignment::Center);
            frame.render_widget(empty, area);
            return;
        }

        // Select first item if nothing is selected
        if self.table_state.selected().is_none() && !aliases.is_empty() {
            self.table_state.select(Some(0));
        }

        // Update scrollbar state with content length
        self.scrollbar_state = self.scrollbar_state.content_length(aliases.len());
        if let Some(selected) = self.table_state.selected() {
            self.scrollbar_state = self.scrollbar_state.position(selected);
        }

        // Create table with modern header and rows
        let header = Row::new(vec![
            Cell::from(" 󰬪 Alias ").style(
                Style::default()
                    .fg(Color::Rgb(236, 72, 153)) // Pink
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from(" 󰆍 Command ").style(
                Style::default()
                    .fg(Color::Rgb(236, 72, 153)) // Pink
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(Style::default().bg(Color::Rgb(30, 40, 50)));

        let rows: Vec<Row<'_>> = aliases
            .iter()
            .enumerate()
            .map(|(idx, (name, value))| {
                let bg = if idx % 2 == 0 {
                    Color::Rgb(20, 20, 30)
                } else {
                    Color::Rgb(25, 25, 35)
                };
                Row::new(vec![
                    Cell::from(format!(" {name} ")).style(
                        Style::default()
                            .fg(Color::Rgb(244, 114, 182)) // Light pink
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Cell::from(format!(" {value} "))
                        .style(Style::default().fg(Color::Rgb(220, 220, 230)).bg(bg)),
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
                .bg(Color::Rgb(236, 72, 153)) // Pink gradient highlight
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
            .style(Style::default().fg(Color::Rgb(236, 72, 153))) // Pink
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
