//! Command history content pane.


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

/// Cached history entry for display.
struct CachedHistoryEntry {
    number: usize,
    command: String,
    formatted_timestamp: String,
}

/// A content pane that displays command history in a scrollable table.
///
/// The history is displayed with the most recent commands at the bottom,
/// matching the traditional bash history display format.
pub struct HistoryPane {
    shell: Arc<tokio::sync::Mutex<brush_core::Shell>>,
    table_state: TableState,
    scrollbar_state: ScrollbarState,
    /// Cached history entries to avoid recomputing on every render.
    cached_entries: Vec<CachedHistoryEntry>,
    /// Whether the cache needs to be refreshed.
    cache_dirty: bool,
}

impl HistoryPane {
    /// Create a new history pane.
    pub fn new(shell: &Arc<tokio::sync::Mutex<brush_core::Shell>>) -> Self {
        Self {
            shell: shell.clone(),
            table_state: TableState::default(),
            scrollbar_state: ScrollbarState::default(),
            cached_entries: Vec::new(),
            cache_dirty: true,
        }
    }

    /// Format a timestamp for display.
    fn format_timestamp(timestamp: Option<brush_core::history::ItemTimestamp>) -> String {
        if let Some(ts) = timestamp {
            // Convert to local time and format nicely
            let local: chrono::DateTime<chrono::Local> = ts.into();
            local.format("%Y-%m-%d %H:%M").to_string()
        } else {
            String::from("—")
        }
    }

    /// Refresh the cached history entries from the shell.
    /// Returns true if the cache was updated, false if the shell was locked.
    fn refresh_cache(&mut self) -> bool {
        let Ok(shell) = self.shell.try_lock() else {
            return false;
        };

        let Some(history) = shell.history() else {
            self.cached_entries.clear();
            self.cache_dirty = false;
            return true;
        };

        let history_count = history.count();

        // Only rebuild if history has actually changed
        if history_count == self.cached_entries.len() && !self.cache_dirty {
            return true;
        }

        let old_count = self.cached_entries.len();

        self.cached_entries = history
            .iter()
            .enumerate()
            .map(|(idx, item)| CachedHistoryEntry {
                number: idx + 1,
                command: item.command_line.clone(),
                formatted_timestamp: Self::format_timestamp(item.timestamp),
            })
            .collect();

        self.cache_dirty = false;

        // If history has grown, auto-scroll to the newest entry
        if history_count > old_count && history_count > 0 {
            self.table_state.select(Some(history_count - 1));
        }

        true
    }
}

impl ContentPane for HistoryPane {
    fn name(&self) -> &'static str {
        "History"
    }

    fn kind(&self) -> PaneKind {
        PaneKind::History
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Refresh cache if needed (only actually rebuilds if history changed)
        if !self.refresh_cache() {
            // Shell is locked (command running), show loading message with modern styling
            let loading = Paragraph::new(" ⏳ Loading history...")
                .style(
                    Style::default()
                        .fg(Color::Rgb(251, 146, 60)) // Orange
                        .bg(Color::Rgb(20, 20, 30))
                        .add_modifier(Modifier::ITALIC),
                )
                .alignment(Alignment::Center);
            frame.render_widget(loading, area);
            return;
        }

        if self.cached_entries.is_empty() {
            let empty = Paragraph::new(" ⚠ No history entries ")
                .style(
                    Style::default()
                        .fg(Color::Rgb(251, 146, 60)) // Orange
                        .bg(Color::Rgb(20, 20, 30)),
                )
                .alignment(Alignment::Center);
            frame.render_widget(empty, area);
            return;
        }

        let history_count = self.cached_entries.len();

        // Select last item if nothing is selected (show most recent by default)
        if self.table_state.selected().is_none() {
            self.table_state
                .select(Some(history_count.saturating_sub(1)));
        }

        // Update scrollbar state with content length
        self.scrollbar_state = self.scrollbar_state.content_length(history_count);
        if let Some(selected) = self.table_state.selected() {
            self.scrollbar_state = self.scrollbar_state.position(selected);
        }

        // Calculate visible row range for virtualization
        // Account for header (1 row) when calculating visible area
        let visible_height = area.height.saturating_sub(1) as usize; // -1 for header
        let selected = self.table_state.selected().unwrap_or(0);

        // Calculate the window of rows to display
        // Keep selected row roughly in the middle when possible
        let half_visible = visible_height / 2;
        let start_idx = if selected > half_visible {
            (selected - half_visible).min(history_count.saturating_sub(visible_height))
        } else {
            0
        };
        let end_idx = (start_idx + visible_height).min(history_count);

        // Adjust selection offset for the windowed view
        let adjusted_selection = selected.saturating_sub(start_idx);

        // Create table with modern header and ONLY visible rows
        let header = Row::new(vec![
            Cell::from(" # ").style(
                Style::default()
                    .fg(Color::Rgb(251, 146, 60)) // Orange
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from(" 󰕗 Time ").style(
                Style::default()
                    .fg(Color::Rgb(251, 146, 60)) // Orange
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from(" 󰆍 Command ").style(
                Style::default()
                    .fg(Color::Rgb(251, 146, 60)) // Orange
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(Style::default().bg(Color::Rgb(30, 40, 50)));

        let rows: Vec<Row<'_>> = self.cached_entries[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let bg = if (start_idx + idx) % 2 == 0 {
                    Color::Rgb(20, 20, 30)
                } else {
                    Color::Rgb(25, 25, 35)
                };
                Row::new(vec![
                    Cell::from(format!(" {} ", entry.number)).style(
                        Style::default()
                            .fg(Color::Rgb(120, 120, 140))
                            .bg(bg)
                            .add_modifier(Modifier::DIM),
                    ),
                    Cell::from(format!(" {} ", entry.formatted_timestamp)).style(
                        Style::default()
                            .fg(Color::Rgb(253, 186, 116)) // Light orange
                            .bg(bg),
                    ),
                    Cell::from(format!(" {} ", entry.command))
                        .style(Style::default().fg(Color::Rgb(220, 220, 230)).bg(bg)),
                ])
            })
            .collect();

        // Create a temporary table state for the windowed view
        let mut windowed_state = TableState::default().with_selected(Some(adjusted_selection));

        let table = Table::new(
            rows,
            [
                Constraint::Length(7),      // History number column
                Constraint::Length(18),     // Timestamp column
                Constraint::Percentage(75), // Command column (takes remaining space)
            ],
        )
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(251, 146, 60)) // Orange gradient highlight
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

        // Render table with windowed state
        frame.render_stateful_widget(table, area, &mut windowed_state);

        // Render modern scrollbar on the right side (still reflects total content)
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::Rgb(251, 146, 60))) // Orange
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        let scrollbar_area = area.inner(Margin {
            vertical: 1, // Leave space for header
            horizontal: 0,
        });

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut self.scrollbar_state);
    }

    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        // Because we use virtualization (only render visible rows with a windowed state),
        // the main table_state doesn't know the total row count. We must manage selection
        // manually with bounds checking.
        let history_count = self.cached_entries.len();
        if history_count == 0 {
            return PaneEventResult::NotHandled;
        }

        let current = self.table_state.selected().unwrap_or(0);
        let max_idx = history_count.saturating_sub(1);

        match event {
            PaneEvent::KeyPress(key, _modifiers) => match key {
                KeyCode::Up => {
                    let new_selection = current.saturating_sub(1);
                    self.table_state.select(Some(new_selection));
                    self.scrollbar_state = self.scrollbar_state.position(new_selection);
                    PaneEventResult::Handled
                }
                KeyCode::Down => {
                    let new_selection = (current + 1).min(max_idx);
                    self.table_state.select(Some(new_selection));
                    self.scrollbar_state = self.scrollbar_state.position(new_selection);
                    PaneEventResult::Handled
                }
                KeyCode::PageUp => {
                    let new_selection = current.saturating_sub(10);
                    self.table_state.select(Some(new_selection));
                    self.scrollbar_state = self.scrollbar_state.position(new_selection);
                    PaneEventResult::Handled
                }
                KeyCode::PageDown => {
                    let new_selection = (current + 10).min(max_idx);
                    self.table_state.select(Some(new_selection));
                    self.scrollbar_state = self.scrollbar_state.position(new_selection);
                    PaneEventResult::Handled
                }
                KeyCode::Home => {
                    self.table_state.select(Some(0));
                    self.scrollbar_state = self.scrollbar_state.position(0);
                    PaneEventResult::Handled
                }
                KeyCode::End => {
                    self.table_state.select(Some(max_idx));
                    self.scrollbar_state = self.scrollbar_state.position(max_idx);
                    PaneEventResult::Handled
                }
                _ => PaneEventResult::NotHandled,
            },
            _ => PaneEventResult::NotHandled,
        }
    }
}
