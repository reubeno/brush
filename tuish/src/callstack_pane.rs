//! Shell call stack content pane.


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

/// A content pane that displays the shell call stack in a scrollable table.
pub struct CallStackPane {
    shell: Arc<tokio::sync::Mutex<brush_core::Shell>>,
    table_state: TableState,
    scrollbar_state: ScrollbarState,
}

impl CallStackPane {
    /// Create a new call stack pane.
    pub fn new(shell: &Arc<tokio::sync::Mutex<brush_core::Shell>>) -> Self {
        Self {
            shell: shell.clone(),
            table_state: TableState::default(),
            scrollbar_state: ScrollbarState::default(),
        }
    }
}

impl ContentPane for CallStackPane {
    fn name(&self) -> &'static str {
        "Call Stack"
    }

    fn kind(&self) -> PaneKind {
        PaneKind::CallStack
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Try to get shell call stack without blocking
        let frames = if let Ok(shell) = self.shell.try_lock() {
            let call_stack = shell.call_stack();
            if call_stack.is_empty() {
                Vec::new()
            } else {
                call_stack
                    .iter()
                    .enumerate()
                    .map(|(index, f)| {
                        let source_info = f.current_pos_as_source_info();
                        let source = source_info.source.to_string();
                        let location = if let Some(pos) = &source_info.start {
                            format!("{}:{}", pos.line, pos.column)
                        } else {
                            String::from("-")
                        };
                        let frame_type = f.frame_type.to_string();
                        (index, source, location, frame_type)
                    })
                    .collect()
            }
        } else {
            // Shell is locked (command running), show loading message
            let loading = ratatui::widgets::Paragraph::new("⏳ Loading call stack...")
                .style(Style::default().fg(Color::White));
            frame.render_widget(loading, area);
            return;
        };

        if frames.is_empty() {
            let empty = ratatui::widgets::Paragraph::new("Call stack is empty")
                .style(Style::default().fg(Color::White));
            frame.render_widget(empty, area);
            return;
        }

        // Select first item if nothing is selected
        if self.table_state.selected().is_none() && !frames.is_empty() {
            self.table_state.select(Some(0));
        }

        // Update scrollbar state with content length
        self.scrollbar_state = self.scrollbar_state.content_length(frames.len());
        if let Some(selected) = self.table_state.selected() {
            self.scrollbar_state = self.scrollbar_state.position(selected);
        }

        // Create table with header and rows
        let header = Row::new(vec![
            Cell::from("#").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Source").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Location").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Type").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().bg(Color::DarkGray));

        let rows: Vec<Row<'_>> = frames
            .iter()
            .map(|(index, source, location, frame_type)| {
                Row::new(vec![
                    Cell::from(format!("{index}")).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(source.as_str()).style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Cell::from(location.as_str()).style(Style::default().fg(Color::Cyan)),
                    Cell::from(frame_type.as_str()).style(Style::default().fg(Color::Magenta)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(4),
                Constraint::Percentage(45),
                Constraint::Percentage(20),
                Constraint::Percentage(30),
            ],
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
