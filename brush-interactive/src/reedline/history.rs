use super::refs;

pub(crate) struct ReedlineHistory {
    pub shell: refs::ShellRef,
}

impl ReedlineHistory {
    fn lock_shell(&self) -> tokio::sync::MutexGuard<'_, brush_core::Shell> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        })
    }
}

impl reedline::History for ReedlineHistory {
    /// Updates or adds a new item to the saved history.
    ///
    /// # Arguments
    ///
    /// * `item` - The history item to save.
    fn save(&mut self, item: reedline::HistoryItem) -> reedline::Result<reedline::HistoryItem> {
        //
        // TODO: Evaluate a way to rationalize between this and the shared
        // history saving. For now, we need to do nothing here to avoid
        // duplicate history items since we are auto-updating the history
        // in a non-reedline-specific way.
        //

        // let brush_item = reedline_history_item_to_brush(&item);
        // let mut shell = self.lock_shell();
        // let history = get_shell_history_mut(&mut shell)?;
        //
        // if let Some(id) = &item.id {
        //     history
        //         .update_by_id(id.0, brush_item)
        //         .map_err(brush_error_to_reedline)?;
        // } else {
        //     let id = history.add(brush_item).map_err(brush_error_to_reedline)?;
        //     item.id = Some(reedline::HistoryItemId(id));
        // }

        Ok(item)
    }

    /// Loads a history item by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the history item to load.
    fn load(&self, id: reedline::HistoryItemId) -> reedline::Result<reedline::HistoryItem> {
        let shell = self.lock_shell();

        // Get the history, retrieve the item, and translate the item it into reedline's format.
        get_shell_history(&shell)?
            .get_by_id(id.0)
            .map_err(brush_error_to_reedline)?
            .ok_or({
                reedline::ReedlineError(reedline::ReedlineErrorVariants::OtherHistoryError(
                    "history item not found",
                ))
            })
            .map(brush_history_item_to_reedline)
    }

    /// Counts all history items matching the given query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query to match against history items.
    #[allow(clippy::significant_drop_tightening)]
    fn count(&self, query: reedline::SearchQuery) -> reedline::Result<i64> {
        let query = reedline_history_query_into_brush(query)?;

        let shell = self.lock_shell();
        let count = get_shell_history(&shell)?.search(query).iter().count();
        drop(shell);

        #[allow(clippy::cast_possible_wrap)]
        Ok(count as i64)
    }

    /// Searches through history, returning all items matching the given query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query to match against history items.
    #[allow(clippy::significant_drop_tightening)]
    fn search(&self, query: reedline::SearchQuery) -> reedline::Result<Vec<reedline::HistoryItem>> {
        let query = reedline_history_query_into_brush(query)?;
        let shell = self.lock_shell();
        let items = get_shell_history(&shell)?
            .search(query)
            .map_err(brush_error_to_reedline)?
            .map(|item| {
                // Translate the item into reedline's format.
                brush_history_item_to_reedline(item)
            })
            .collect::<Vec<_>>();

        Ok(items)
    }

    /// Update a history item.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the history item to update.
    /// * `updater` - A function that takes a history item and returns an updated history item.
    fn update(
        &mut self,
        id: reedline::HistoryItemId,
        updater: &dyn Fn(reedline::HistoryItem) -> reedline::HistoryItem,
    ) -> reedline::Result<()> {
        // TODO: Understand atomicity expectations of reedline.
        let item = self.load(id)?;
        let updated_item = updater(item);
        self.save(updated_item)?;

        Ok(())
    }

    /// Delete all history items.
    fn clear(&mut self) -> reedline::Result<()> {
        let mut shell = self.lock_shell();

        // Get the history, retrieve the item, and translate the item it into reedline's format.
        get_shell_history_mut(&mut shell)?
            .clear()
            .map_err(brush_error_to_reedline)
    }

    /// Delete the history item with the given ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the history item to delete.
    fn delete(&mut self, id: reedline::HistoryItemId) -> reedline::Result<()> {
        let mut shell = self.lock_shell();

        get_shell_history_mut(&mut shell)?
            .delete_item_by_id(id.0)
            .map_err(brush_error_to_reedline)
    }

    /// Sync all history items to backing storage.
    fn sync(&mut self) -> std::io::Result<()> {
        let mut shell = self.lock_shell();
        shell.save_history().map_err(std::io::Error::other)
    }

    /// Retrieves a unique ID for the current session.
    fn session(&self) -> Option<reedline::HistorySessionId> {
        // Not implemented for now.
        None
    }
}

fn brush_history_item_to_reedline(item: &brush_core::history::Item) -> reedline::HistoryItem {
    let mut rl_item = reedline::HistoryItem::from_command_line(item.command_line.as_str());
    rl_item.id = Some(reedline::HistoryItemId(item.id));
    rl_item.start_timestamp = item.timestamp;

    rl_item
}

#[allow(unused)]
fn reedline_history_item_to_brush(item: &reedline::HistoryItem) -> brush_core::history::Item {
    // TODO: implement more fields when they are added to Item
    brush_core::history::Item {
        id: item.id.map_or(0, |id| id.0),
        command_line: item.command_line.clone(),
        timestamp: item.start_timestamp,
        dirty: true,
    }
}

fn brush_error_to_reedline(error: brush_core::Error) -> reedline::ReedlineError {
    reedline::ReedlineError::from(std::io::Error::other(error))
}

#[allow(clippy::unnecessary_wraps)]
fn reedline_history_query_into_brush(
    query: reedline::SearchQuery,
) -> reedline::Result<brush_core::history::Query> {
    let mut result = brush_core::history::Query {
        direction: match query.direction {
            reedline::SearchDirection::Forward => brush_core::history::Direction::Forward,
            reedline::SearchDirection::Backward => brush_core::history::Direction::Backward,
        },
        max_items: query.limit,
        not_at_or_before_id: if matches!(query.direction, reedline::SearchDirection::Backward) {
            query.end_id.map(|id| id.0)
        } else {
            query.start_id.map(|id| id.0)
        },
        not_at_or_after_id: if matches!(query.direction, reedline::SearchDirection::Backward) {
            query.start_id.map(|id| id.0)
        } else {
            query.end_id.map(|id| id.0)
        },
        not_at_or_before_time: if matches!(query.direction, reedline::SearchDirection::Backward) {
            query.end_time
        } else {
            query.start_time
        },
        not_at_or_after_time: if matches!(query.direction, reedline::SearchDirection::Backward) {
            query.start_time
        } else {
            query.end_time
        },
        ..Default::default()
    };

    if let Some(cmdline_filter) = query.filter.command_line {
        result.command_line_filter = match cmdline_filter {
            reedline::CommandLineSearch::Exact(cmdline) => {
                Some(brush_core::history::CommandLineFilter::Exact(cmdline))
            }
            reedline::CommandLineSearch::Substring(cmdline) => {
                Some(brush_core::history::CommandLineFilter::Contains(cmdline))
            }
            reedline::CommandLineSearch::Prefix(cmdline) => {
                Some(brush_core::history::CommandLineFilter::Prefix(cmdline))
            }
        }
    }

    if query.filter.cwd_exact.is_some()
        || query.filter.cwd_prefix.is_some()
        || query.filter.exit_successful.is_some()
        || query.filter.hostname.is_some()
        || query.filter.session.is_some()
    {
        return Err(reedline::ReedlineError(
            reedline::ReedlineErrorVariants::HistoryFeatureUnsupported {
                history: "(default)",
                feature: "search filter",
            },
        ));
    }

    Ok(result)
}

fn get_shell_history<'a>(
    shell: &'a tokio::sync::MutexGuard<'_, brush_core::Shell>,
) -> Result<&'a brush_core::history::History, reedline::ReedlineError> {
    shell.history().ok_or({
        reedline::ReedlineError(reedline::ReedlineErrorVariants::HistoryFeatureUnsupported {
            history: "(default)",
            feature: "load",
        })
    })
}

fn get_shell_history_mut<'a>(
    shell: &'a mut tokio::sync::MutexGuard<'_, brush_core::Shell>,
) -> Result<&'a mut brush_core::history::History, reedline::ReedlineError> {
    shell.history_mut().ok_or({
        reedline::ReedlineError(reedline::ReedlineErrorVariants::HistoryFeatureUnsupported {
            history: "(default)",
            feature: "load",
        })
    })
}
