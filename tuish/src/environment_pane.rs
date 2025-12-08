//! Environment variables content pane.

#![allow(dead_code)]

use crossterm::event::KeyCode;
use ratatui::{
    prelude::*,
    widgets::{Cell, Row, Table},
};

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult};

/// A content pane that displays environment variables in a scrollable table.
pub struct EnvironmentPane {
    variables: Vec<(String, String)>,
    scroll_offset: usize,
}

impl EnvironmentPane {
    /// Create a new environment pane.
    pub fn new() -> Self {
        Self {
            variables: Vec::new(),
            scroll_offset: 0,
        }
    }

    /// Updates the environment variables displayed in this pane.
    pub fn update_variables(&mut self, mut vars: Vec<(String, String)>) {
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        self.variables = vars;
        // Reset scroll if we're past the end
        self.clamp_scroll(0);
    }

    /// Clamps scroll offset based on available height.
    fn clamp_scroll(&mut self, available_height: u16) {
        let max_scroll = self
            .variables
            .len()
            .saturating_sub(available_height as usize);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }
}

impl ContentPane for EnvironmentPane {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn name(&self) -> &str {
        "Environment"
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if self.variables.is_empty() {
            let loading = ratatui::widgets::Paragraph::new("Loading environment variables...")
                .style(Style::default().fg(Color::White));
            frame.render_widget(loading, area);
            return;
        }

        // Create table with header and rows
        let header = Row::new(vec![
            Cell::from("Variable").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Value").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().bg(Color::DarkGray));

        // Clamp scroll offset to prevent scrolling past content
        let available_height = area.height.saturating_sub(2); // Subtract header and margin
        self.clamp_scroll(available_height);

        // Skip rows based on scroll offset
        let rows = self.variables.iter().skip(self.scroll_offset).map(|(k, v)| {
            Row::new(vec![
                Cell::from(k.as_str()).style(Style::default().add_modifier(Modifier::ITALIC)),
                Cell::from(v.as_str()),
            ])
        });

        let table = Table::new(rows, [Constraint::Percentage(30), Constraint::Percentage(70)])
            .header(header)
            .style(Style::default().fg(Color::White));

        frame.render_widget(table, area);
    }

    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        match event {
            PaneEvent::KeyPress(key, _modifiers) => match key {
                KeyCode::Up => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    PaneEventResult::Handled
                }
                KeyCode::Down => {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                    PaneEventResult::Handled
                }
                KeyCode::PageUp => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(10);
                    PaneEventResult::Handled
                }
                KeyCode::PageDown => {
                    self.scroll_offset = self.scroll_offset.saturating_add(10);
                    PaneEventResult::Handled
                }
                KeyCode::Home => {
                    self.scroll_offset = 0;
                    PaneEventResult::Handled
                }
                KeyCode::End => {
                    self.scroll_offset = usize::MAX; // Will be clamped on next render
                    PaneEventResult::Handled
                }
                _ => PaneEventResult::NotHandled,
            },
            _ => PaneEventResult::NotHandled,
        }
    }
}
