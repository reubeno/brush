//! Facilities for tracking and persisting the shell's command history.

use chrono::Utc;
use std::{
    collections::BTreeSet,
    io::{BufRead, Read, Write},
    path::Path,
};

use crate::error;

/// Represents a unique identifier for a history item.
type ItemId = i64;

/// Interface for querying and manipulating the shell's recorded history of commands.
// TODO(history): support maximum item count
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct History {
    /// The items, oldest first. Their IDs only increase, so they're in ID order too, which
    /// finds an item by ID (by binary search) without a separate map from IDs to items.
    items: rpds::VectorSync<Item>,
    next_id: ItemId,
}

impl History {
    /// Constructs a new `History` instance, with its contents initialized from the given readable
    /// stream. If errors are encountered reading lines from the stream, unreadable lines will
    /// be skipped but the call will still return successfully, with a warning logged. An error
    /// result will be returned only if an internal error occurs updating the history. Like
    /// bash, an entry the stream gives no timestamp gets the time it's read.
    ///
    /// # Arguments
    ///
    /// * `reader` - The readable stream to import history from.
    pub fn import(reader: impl Read) -> Result<Self, error::Error> {
        let mut history = Self::default();
        let read_time = Utc::now();

        let buf_reader = std::io::BufReader::new(reader);

        let mut next_timestamp = None;
        for line_result in buf_reader.lines() {
            let line = match line_result {
                Ok(line) => line,
                // If we couldn't decode the line due to invalid data (perhaps it wasn't
                // valid UTF8?), skip it and make a best-effort attempt to proceed on.
                // We'll later warn the user.
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                    tracing::warn!("unreadable history line; {err}");
                    continue;
                }
                // In the event of other kinds of errors, return an error result. We don't
                // want to get stuck in a failing I/O loop.
                Err(err) => {
                    return Err(err.into());
                }
            };

            // Look for timestamp comments; ignore other comment lines.
            if let Some(comment) = line.strip_prefix("#") {
                if let Ok(seconds_since_epoch) = comment.trim().parse() {
                    next_timestamp = ItemTimestamp::from_timestamp(seconds_since_epoch, 0);
                } else {
                    next_timestamp = None;
                }

                continue;
            }

            let item = Item {
                id: history.next_id,
                command_line: line,
                timestamp: Some(next_timestamp.take().unwrap_or(read_time)),
                dirty: false,
            };

            history.add(item)?;
        }

        Ok(history)
    }

    /// Tries to retrieve a history item by its unique identifier. Returns `None` if no item is
    /// found.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the history item to retrieve.
    pub fn get_by_id(&self, id: ItemId) -> Result<Option<&Item>, error::Error> {
        Ok(self.position_of(id).and_then(|index| self.items.get(index)))
    }

    /// Replaces the history item with the given ID with a new item. Returns an error if the item
    /// cannot be updated.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the history item to update.
    /// * `item` - The new history item to replace the old one.
    pub fn update_by_id(&mut self, id: ItemId, item: Item) -> Result<(), error::Error> {
        let index = self
            .position_of(id)
            .ok_or(error::ErrorKind::HistoryItemNotFound)?;
        // The item keeps its ID, so the items stay in ID order.
        self.items.set_mut(index, Item { id, ..item });
        Ok(())
    }

    /// Removes the nth item from the history (counting from 0, the oldest). Returns whether
    /// there was such an item to remove.
    pub fn remove_nth_item(&mut self, n: usize) -> bool {
        let len = self.items.len();
        if n >= len {
            return false;
        }

        // Take off the items from the nth on, and put back those after it: so removing a
        // recent item (as `fc -s` does its own) touches only the few after it.
        let after: Vec<Item> = (n + 1..len)
            .filter_map(|i| self.items.get(i).cloned())
            .collect();
        for _ in n..len {
            self.items.drop_last_mut();
        }
        for item in after {
            self.items.push_back_mut(item);
        }

        true
    }

    /// Keeps only the items that `keep` returns true for, in order. Their IDs don't change.
    pub fn retain(&mut self, mut keep: impl FnMut(&Item) -> bool) {
        let Some(first_removed) = self.items.iter().position(|item| !keep(item)) else {
            return;
        };

        // As in `remove_nth_item`, rebuild only from the first item removed on.
        let kept: Vec<Item> = self
            .items
            .iter()
            .skip(first_removed + 1)
            .filter(|item| keep(item))
            .cloned()
            .collect();
        for _ in first_removed..self.items.len() {
            self.items.drop_last_mut();
        }
        for item in kept {
            self.items.push_back_mut(item);
        }
    }

    /// Adds a new history item. Returns the unique identifier of the newly added item.
    ///
    /// # Arguments
    ///
    /// * `item` - The history item to add.
    pub fn add(&mut self, mut item: Item) -> Result<ItemId, error::Error> {
        let id = self.next_id;

        item.id = id;
        self.next_id += 1;

        self.items.push_back_mut(item);

        Ok(id)
    }

    /// Deletes a history item by its unique identifier. Returns an error if the item cannot be
    /// deleted.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the history item to delete.
    pub fn delete_item_by_id(&mut self, id: ItemId) -> Result<(), error::Error> {
        if let Some(index) = self.position_of(id) {
            self.remove_nth_item(index);
        }

        Ok(())
    }

    /// Clears all history items.
    pub fn clear(&mut self) -> Result<(), error::Error> {
        self.items = rpds::VectorSync::new_sync();
        Ok(())
    }

    /// Flushes the history to backing storage (if relevant).
    ///
    /// # Arguments
    ///
    /// * `history_file_path` - The path to the history file.
    /// * `append` - Whether to append to the file or overwrite it.
    /// * `unsaved_items_only` - Whether to only write unsaved items; if true, any items will be
    ///   marked as "saved" once saved.
    /// * `write_timestamps` - Whether to write timestamps for each command line.
    pub fn flush(
        &mut self,
        history_file_path: impl AsRef<Path>,
        append: bool,
        unsaved_items_only: bool,
        write_timestamps: bool,
    ) -> Result<(), error::Error> {
        // Open the file
        let mut file_options = std::fs::File::options();

        if append {
            file_options.append(true);
        } else {
            file_options.write(true).truncate(true);
        }

        let mut file =
            std::io::BufWriter::new(file_options.create(true).open(history_file_path.as_ref())?);

        let mut written = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            if unsaved_items_only && !item.dirty {
                continue;
            }

            if write_timestamps && let Some(timestamp) = item.timestamp {
                writeln!(file, "#{}", timestamp.timestamp())?;
            }

            writeln!(file, "{}", item.command_line)?;

            if unsaved_items_only {
                written.push(index);
            }
        }

        // Only once they're all written out, mark the items saved: if writing fails, they
        // stay unsaved, so a later flush retries them.
        file.into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;
        for index in written {
            if let Some(item) = self.items.get_mut(index) {
                item.dirty = false;
            }
        }

        Ok(())
    }

    /// Searches through history using the given query.
    ///
    /// # Arguments
    ///
    /// * `query` - The query to use.
    pub fn search(&self, query: Query) -> Result<impl Iterator<Item = &self::Item>, error::Error> {
        Ok(Search::new(self, query))
    }

    /// Returns whether the history contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns an iterator over the history items.
    pub fn iter(&self) -> impl Iterator<Item = &self::Item> {
        self.items.iter()
    }

    /// Retrieves the nth history item, if it exists. Returns `None` if no such item exists.
    /// Indexing is zero-based, with an index of 0 referencing the oldest item in the history.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the history item to retrieve.
    pub fn get(&self, index: usize) -> Option<&Item> {
        self.items.get(index)
    }

    /// Returns the newest item, if there is one.
    pub fn last(&self) -> Option<&Item> {
        self.items.last()
    }

    /// Returns the number of items in the history.
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Returns how many items have IDs less than `id`: the position of the item with that ID,
    /// if there is one.
    fn position_before(&self, id: ItemId) -> usize {
        // The items are in ID order, so binary search. (By hand: `slice::partition_point`
        // needs a slice, which rpds's vector, a tree, isn't.)
        let (mut low, mut high) = (0, self.items.len());
        while low < high {
            let mid = low + (high - low) / 2;
            if self.items.get(mid).is_some_and(|item| item.id < id) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }

    /// Returns the position of the item with the given ID, if there is one.
    fn position_of(&self, id: ItemId) -> Option<usize> {
        let index = self.position_before(id);
        self.items
            .get(index)
            .is_some_and(|item| item.id == id)
            .then_some(index)
    }
}

/// A value of `HISTCONTROL`, which controls what an interactive shell saves to history.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, strum_macros::EnumString)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum HistoryControl {
    /// Don't save a command that starts with a space.
    IgnoreSpace,
    /// Don't save a command that's the same as the last one saved.
    IgnoreDups,
    /// Both `ignorespace` and `ignoredups`.
    IgnoreBoth,
    /// Before saving a command, remove any earlier ones the same as it.
    EraseDups,
}

impl HistoryControl {
    /// Parses `HISTCONTROL`'s colon-separated values into the set of them, with `ignoreboth`
    /// as the two it stands for. Like bash, values it doesn't know are ignored.
    pub(crate) fn parse_list(value: &str) -> BTreeSet<Self> {
        value
            .split(':')
            .filter_map(|value| value.parse().ok())
            .flat_map(|control| match control {
                Self::IgnoreBoth => [Self::IgnoreSpace, Self::IgnoreDups].as_slice(),
                Self::IgnoreSpace => &[Self::IgnoreSpace],
                Self::IgnoreDups => &[Self::IgnoreDups],
                Self::EraseDups => &[Self::EraseDups],
            })
            .copied()
            .collect()
    }
}

/// Represents a timestamp for a history item.
pub type ItemTimestamp = chrono::DateTime<Utc>;

/// Represents an item in the history.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Item {
    /// The unique identifier of the history item.
    pub id: ItemId,
    /// The actual command line.
    pub command_line: String,
    /// The timestamp when the command was started.
    pub timestamp: Option<ItemTimestamp>,
    /// Whether or not the item is dirty, i.e., has not yet been written to backing storage.
    pub dirty: bool,
}

impl Item {
    /// Constructs a new `Item` with the given command line.
    ///
    /// # Arguments
    ///
    /// * `command_line` - The command line of the item.
    pub fn new(command_line: impl Into<String>) -> Self {
        Self {
            id: 0, // NOTE: ID will be assigned when added to the history.
            command_line: command_line.into(),
            timestamp: Some(chrono::Utc::now()),
            dirty: true,
        }
    }
}

/// Encapsulates query parameters for searching through history.
#[derive(Default)]
pub struct Query {
    /// Whether to search forward or backward
    pub direction: Direction,
    /// Optionally, clamp results to items with a timestamp strictly after this.
    pub not_at_or_before_time: Option<ItemTimestamp>,
    /// Optionally, clamp results to items with a timestamp strictly before this.
    pub not_at_or_after_time: Option<ItemTimestamp>,
    /// Optionally, clamp results to items with an ID equal strictly after this.
    pub not_at_or_before_id: Option<ItemId>,
    /// Optionally, clamp results to items with an ID equal strictly before this.
    pub not_at_or_after_id: Option<ItemId>,
    /// Optionally, maximum number of items to retrieve
    pub max_items: Option<i64>,
    /// Optionally, a string-based filter on command line.
    pub command_line_filter: Option<CommandLineFilter>,
}

impl Query {
    /// Checks if the query includes the given item.
    ///
    /// # Arguments
    ///
    /// * `item` - The item to check.
    pub fn includes(&self, item: &Item) -> bool {
        // Filter based on not_at_or_before_time.
        if let Some(not_at_or_before_time) = &self.not_at_or_before_time {
            if item
                .timestamp
                .is_some_and(|ts| ts <= *not_at_or_before_time)
            {
                return false;
            }
        }

        // Filter based on not_at_or_after_time
        if let Some(not_at_or_after_time) = &self.not_at_or_after_time {
            if item.timestamp.is_some_and(|ts| ts >= *not_at_or_after_time) {
                return false;
            }
        }

        // Filter based on not_at_or_before_id
        if self
            .not_at_or_before_id
            .is_some_and(|query_id| item.id <= query_id)
        {
            return false;
        }

        // Filter based on not_at_or_after_id
        if self
            .not_at_or_after_id
            .is_some_and(|query_id| item.id >= query_id)
        {
            return false;
        }

        // Filter based on command_line_filter
        if let Some(command_line_filter) = &self.command_line_filter {
            match command_line_filter {
                CommandLineFilter::Prefix(prefix) => {
                    if !item.command_line.starts_with(prefix) {
                        return false;
                    }
                }
                CommandLineFilter::Suffix(suffix) => {
                    if !item.command_line.ends_with(suffix) {
                        return false;
                    }
                }
                CommandLineFilter::Contains(contains) => {
                    if !item.command_line.contains(contains) {
                        return false;
                    }
                }
                CommandLineFilter::Exact(exact) => {
                    if item.command_line != *exact {
                        return false;
                    }
                }
            }
        }

        true
    }
}

/// Represents the direction of a search operation.
#[derive(Default)]
pub enum Direction {
    /// Search forward from the oldest part of history.
    #[default]
    Forward,
    /// Search backward from the youngest part of history.
    Backward,
}

/// Filter criteria for command lines.
pub enum CommandLineFilter {
    /// The command line must start with this string.
    Prefix(String),
    /// The command line must end with this string.
    Suffix(String),
    /// The command line must contain this string.
    Contains(String),
    /// The command line must match this string exactly.
    Exact(String),
}

/// Represents a search operation.
pub struct Search<'a> {
    /// The history to search through.
    history: &'a History,
    /// The positions of the items left to search: those within the query's ID bounds.
    positions: std::ops::Range<usize>,
    /// The query to apply.
    query: Query,
    /// Count of items returned so far.
    count: usize,
}

impl<'a> Search<'a> {
    /// Constructs a new search against the provided history, querying *all* items.
    ///
    /// # Arguments
    ///
    /// * `history` - The history to search through.
    pub fn all(history: &'a History) -> Self {
        Self::new(history, Query::default())
    }

    /// Constructs a new search against the provided history, using the given query.
    ///
    /// # Arguments
    ///
    /// * `history` - The history to search through.
    /// * `query` - The query to use.
    pub fn new(history: &'a History, query: Query) -> Self {
        // The items within the query's ID bounds are a run of them, since they're in ID
        // order: search just those positions.
        let start = query.not_at_or_before_id.map_or(0, |id| {
            id.checked_add(1)
                .map_or(history.items.len(), |id| history.position_before(id))
        });
        let end = query
            .not_at_or_after_id
            .map_or(history.items.len(), |id| history.position_before(id));

        Self {
            history,
            positions: start..end.max(start),
            query,
            count: 0,
        }
    }
}

impl<'a> Iterator for Search<'a> {
    type Item = &'a Item;

    fn next(&mut self) -> Option<Self::Item> {
        // Once we hit the limit, we stop searching.
        #[expect(clippy::cast_possible_truncation)]
        #[expect(clippy::cast_sign_loss)]
        if self
            .query
            .max_items
            .is_some_and(|max_items| self.count >= max_items as usize)
        {
            return None;
        }

        loop {
            let index = match self.query.direction {
                Direction::Forward => self.positions.next(),
                Direction::Backward => self.positions.next_back(),
            }?;
            let item = self.history.items.get(index)?;
            if self.query.includes(item) {
                self.count += 1;
                return Some(item);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use super::*;

    fn history(lines: &[&str]) -> Result<History, error::Error> {
        let mut history = History::default();
        for line in lines {
            history.add(Item::new(*line))?;
        }
        Ok(history)
    }

    fn command_lines<'a>(items: impl Iterator<Item = &'a Item>) -> Vec<&'a str> {
        items.map(|item| item.command_line.as_str()).collect()
    }

    /// Removing an item shifts the positions of those after it (as `history -d` shows), but
    /// no item's ID changes: an ID names its item for good.
    #[test]
    fn ids_are_stable_across_removal() -> Result<(), error::Error> {
        let mut history = history(&["a", "b", "c", "d"])?;

        assert!(history.remove_nth_item(1));
        assert_eq!(
            history.get(1).map(|item| item.command_line.as_str()),
            Some("c")
        );
        assert_eq!(
            history.get_by_id(2)?.map(|item| item.command_line.as_str()),
            Some("c")
        );

        history.delete_item_by_id(0)?;
        assert_eq!(command_lines(history.iter()), ["c", "d"]);
        assert!(history.get_by_id(0)?.is_none());
        assert!(history.get_by_id(1)?.is_none());
        assert_eq!(history.get_by_id(3)?.map(|item| item.id), Some(3));

        // New items still get new IDs.
        assert_eq!(history.add(Item::new("e"))?, 4);
        assert_eq!(
            history.get_by_id(4)?.map(|item| item.command_line.as_str()),
            Some("e")
        );

        // Even after clearing history: an old ID never comes to name a new item.
        history.clear()?;
        assert_eq!(history.add(Item::new("f"))?, 5);
        assert!(history.get_by_id(0)?.is_none());

        Ok(())
    }

    /// Updating an item keeps its ID, whatever ID the new item carries.
    #[test]
    fn update_by_id_keeps_the_id() -> Result<(), error::Error> {
        let mut history = history(&["a", "b"])?;

        history.update_by_id(
            1,
            Item {
                id: 99,
                ..Item::new("x")
            },
        )?;
        let updated = history
            .get_by_id(1)?
            .map(|item| (item.id, item.command_line.as_str()));
        assert_eq!(updated, Some((1, "x")));
        assert!(history.get_by_id(99)?.is_none());

        assert!(history.update_by_id(7, Item::new("y")).is_err());

        Ok(())
    }

    #[test]
    fn search_by_direction_bounds_and_prefix() -> Result<(), error::Error> {
        let history = history(&["echo hi there", "ls", "echo hello world", "cd /tmp"])?;
        let search = |query| history.search(query).map(command_lines);
        let prefix = |text: &str| Some(CommandLineFilter::Prefix(text.to_owned()));

        // The newest match, as a hint asks for...
        let newest = Query {
            direction: Direction::Backward,
            max_items: Some(1),
            command_line_filter: prefix("echo h"),
            ..Query::default()
        };
        assert_eq!(search(newest)?, ["echo hello world"]);

        // ...and older than a given ID, as history navigation asks for.
        let older = Query {
            direction: Direction::Backward,
            not_at_or_after_id: Some(2),
            command_line_filter: prefix("echo h"),
            ..Query::default()
        };
        assert_eq!(search(older)?, ["echo hi there"]);

        let newer = Query {
            not_at_or_before_id: Some(0),
            not_at_or_after_id: Some(3),
            ..Query::default()
        };
        assert_eq!(search(newer)?, ["ls", "echo hello world"]);

        // Newer than a given ID, with no upper bound, as moving down through history asks.
        let down = Query {
            not_at_or_before_id: Some(1),
            ..Query::default()
        };
        assert_eq!(search(down)?, ["echo hello world", "cd /tmp"]);

        // By substring (as Ctrl-R searches) or exact match, or with no room for any.
        let substring = Query {
            direction: Direction::Backward,
            command_line_filter: Some(CommandLineFilter::Contains("hel".to_owned())),
            ..Query::default()
        };
        assert_eq!(search(substring)?, ["echo hello world"]);
        let exact = Query {
            command_line_filter: Some(CommandLineFilter::Exact("ls".to_owned())),
            ..Query::default()
        };
        assert_eq!(search(exact)?, ["ls"]);
        let no_room = Query {
            max_items: Some(0),
            ..Query::default()
        };
        assert_eq!(search(no_room)?, Vec::<&str>::new());

        let none = Query {
            direction: Direction::Backward,
            command_line_filter: prefix("echo hix"),
            ..Query::default()
        };
        assert_eq!(search(none)?, Vec::<&str>::new());

        Ok(())
    }

    /// A search finds just what checking each item would, in order, whatever its direction,
    /// ID bounds, limit, and filter -- including bounds at deleted IDs and at the extremes.
    /// And an item is found by its ID just as a scan would find it.
    #[test]
    fn search_and_get_by_id_match_a_scan() -> Result<(), error::Error> {
        for len in 0..6_i64 {
            for deleted in 0..(1 << len) {
                let mut history = History::default();
                for id in 0..len {
                    history.add(Item::new(if id % 2 == 0 { "even" } else { "odd" }))?;
                }
                for id in (0..len).filter(|id| deleted & (1 << id) != 0) {
                    history.delete_item_by_id(id)?;
                }
                let scan = || (0..history.count()).filter_map(|index| history.get(index));

                let bounds = [None, Some(ItemId::MIN), Some(-1), Some(0), Some(2)]
                    .into_iter()
                    .chain([Some(len - 1), Some(len), Some(ItemId::MAX)]);
                for lower in bounds.clone() {
                    for upper in bounds.clone() {
                        for backward in [false, true] {
                            for max_items in [None, Some(0), Some(1), Some(2), Some(-1)] {
                                for filter in [None, Some("odd")] {
                                    let query = || Query {
                                        direction: if backward {
                                            Direction::Backward
                                        } else {
                                            Direction::Forward
                                        },
                                        not_at_or_before_id: lower,
                                        not_at_or_after_id: upper,
                                        max_items,
                                        command_line_filter: filter
                                            .map(|f| CommandLineFilter::Prefix(f.to_owned())),
                                        ..Query::default()
                                    };

                                    let found: Vec<ItemId> =
                                        history.search(query())?.map(|item| item.id).collect();
                                    let mut expected: Vec<ItemId> = scan()
                                        .filter(|item| query().includes(item))
                                        .map(|item| item.id)
                                        .collect();
                                    if backward {
                                        expected.reverse();
                                    }
                                    // A negative limit is no limit.
                                    if let Some(max) =
                                        max_items.and_then(|m| usize::try_from(m).ok())
                                    {
                                        expected.truncate(max);
                                    }
                                    assert_eq!(
                                        found, expected,
                                        "len {len}, deleted {deleted:b}, bounds {lower:?}..{upper:?}, backward {backward}, max {max_items:?}, filter {filter:?}"
                                    );
                                }
                            }
                        }
                    }
                }

                for id in -1..=len {
                    let by_scan = scan().find(|item| item.id == id).map(|item| item.id);
                    assert_eq!(history.get_by_id(id)?.map(|item| item.id), by_scan);
                }
            }
        }

        Ok(())
    }

    /// Appending unsaved items writes just those, once; writing everything writes it all.
    #[test]
    fn flush_writes_unsaved_items_once() -> Result<(), error::Error> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("history");
        let read = || std::fs::read_to_string(&path);

        // Imported items are already saved; added ones aren't.
        let mut history = History::import(&b"old\n"[..])?;
        history.add(Item::new("new1"))?;
        history.add(Item::new("new2"))?;

        history.flush(&path, true, true, false)?;
        assert_eq!(read()?, "new1\nnew2\n");
        history.flush(&path, true, true, false)?;
        assert_eq!(read()?, "new1\nnew2\n");

        history.flush(&path, false, false, false)?;
        assert_eq!(read()?, "old\nnew1\nnew2\n");

        Ok(())
    }

    /// Each of a query's filters, on its own. ID and time bounds are exclusive, and an item
    /// with no timestamp isn't excluded by time bounds.
    #[test]
    fn query_includes_by_each_filter() {
        let at = |seconds| ItemTimestamp::from_timestamp(seconds, 0);
        let item = |id, command_line: &str, timestamp| Item {
            id,
            command_line: command_line.to_owned(),
            timestamp,
            dirty: false,
        };

        assert!(Query::default().includes(&item(5, "echo hi", None)));

        let after_id = Query {
            not_at_or_before_id: Some(2),
            ..Query::default()
        };
        assert!(!after_id.includes(&item(2, "x", None)));
        assert!(after_id.includes(&item(3, "x", None)));
        let before_id = Query {
            not_at_or_after_id: Some(2),
            ..Query::default()
        };
        assert!(!before_id.includes(&item(2, "x", None)));
        assert!(before_id.includes(&item(1, "x", None)));

        let after_time = Query {
            not_at_or_before_time: at(100),
            ..Query::default()
        };
        assert!(!after_time.includes(&item(0, "x", at(100))));
        assert!(after_time.includes(&item(0, "x", at(101))));
        assert!(after_time.includes(&item(0, "x", None)));
        let before_time = Query {
            not_at_or_after_time: at(100),
            ..Query::default()
        };
        assert!(!before_time.includes(&item(0, "x", at(100))));
        assert!(before_time.includes(&item(0, "x", at(99))));
        assert!(before_time.includes(&item(0, "x", None)));

        for (filter, matching, other) in [
            (CommandLineFilter::Prefix("ec".to_owned()), "echo", "sec"),
            (CommandLineFilter::Suffix("ho".to_owned()), "echo", "hot"),
            (CommandLineFilter::Contains("ch".to_owned()), "echo", "cd"),
            (CommandLineFilter::Exact("echo".to_owned()), "echo", "echo "),
        ] {
            let query = Query {
                command_line_filter: Some(filter),
                ..Query::default()
            };
            assert!(query.includes(&item(0, matching, None)), "{matching:?}");
            assert!(!query.includes(&item(0, other, None)), "{other:?}");
        }
    }

    /// An empty history, positions past the end, and IDs no item has.
    #[test]
    fn empty_history_and_missing_items() -> Result<(), error::Error> {
        let mut empty = History::default();
        assert!(empty.is_empty());
        assert_eq!(empty.count(), 0);
        assert!(empty.get(0).is_none());
        assert!(empty.get_by_id(0)?.is_none());
        assert!(!empty.remove_nth_item(0));
        empty.delete_item_by_id(0)?;
        assert!(empty.update_by_id(0, Item::new("x")).is_err());
        assert_eq!(empty.iter().count(), 0);
        assert_eq!(empty.search(Query::default())?.count(), 0);

        let mut history = history(&["a", "b", "c"])?;
        assert!(!history.is_empty());
        assert_eq!(history.count(), 3);
        assert!(history.get(3).is_none());
        assert!(!history.remove_nth_item(3));
        history.delete_item_by_id(7)?;
        assert_eq!(command_lines(history.iter()), ["a", "b", "c"]);

        // Updating an item changes just it, where it is.
        history.update_by_id(1, Item::new("B"))?;
        assert_eq!(command_lines(history.iter()), ["a", "B", "c"]);
        assert_eq!(command_lines(Search::all(&history)), ["a", "B", "c"]);

        Ok(())
    }

    /// A new item is unsaved and stamped now; adding it gives it the next ID.
    #[test]
    fn new_items_are_unsaved_and_stamped() -> Result<(), error::Error> {
        let before = Utc::now();
        let item = Item::new("ls");
        assert!(item.dirty);
        assert!(item.timestamp.is_some_and(|ts| ts >= before));

        let mut history = history(&["a"])?;
        assert_eq!(history.add(item)?, 1);
        assert_eq!(history.get(1).map(|item| item.id), Some(1));

        Ok(())
    }

    /// Importing reads command lines and the timestamp comments before them (the last of
    /// several; a comment that isn't one clears it), skips unreadable lines, and leaves the
    /// items saved and numbered from 0.
    #[test]
    fn import_reads_history_files() -> Result<(), error::Error> {
        let before = Utc::now();
        let file = b"#1700000000\na\n#1700000005\n#not a time\nb\n#1700000001\n#1700000002\nc\n\xff\xfe\nd\n";
        let mut history = History::import(&file[..])?;

        assert_eq!(command_lines(history.iter()), ["a", "b", "c", "d"]);
        let timestamps: Vec<_> = history.iter().map(|item| item.timestamp).collect();
        assert_eq!(
            timestamps[0],
            ItemTimestamp::from_timestamp(1_700_000_000, 0)
        );
        assert!(timestamps[1].is_some_and(|ts| ts >= before));
        assert_eq!(
            timestamps[2],
            ItemTimestamp::from_timestamp(1_700_000_002, 0)
        );
        assert!(timestamps[3].is_some_and(|ts| ts >= before));
        assert!(history.iter().all(|item| !item.dirty));
        assert_eq!(
            history.iter().map(|item| item.id).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(history.add(Item::new("e"))?, 4);

        assert!(History::import(&b""[..])?.is_empty());

        Ok(())
    }

    /// Flushing appends to what's already in the file, with the timestamps of items that
    /// have them if asked; importing that reads them back.
    #[test]
    fn flush_and_import_round_trip_with_timestamps() -> Result<(), error::Error> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("history");
        std::fs::write(&path, "existing\n")?;

        let mut history = History::default();
        history.add(Item {
            timestamp: ItemTimestamp::from_timestamp(1_700_000_000, 0),
            ..Item::new("a")
        })?;
        history.add(Item {
            timestamp: None,
            ..Item::new("b")
        })?;
        history.flush(&path, true, true, true)?;
        assert_eq!(
            std::fs::read_to_string(&path)?,
            "existing\n#1700000000\na\nb\n"
        );

        let reread = History::import(std::fs::File::open(&path)?)?;
        assert_eq!(command_lines(reread.iter()), ["existing", "a", "b"]);
        assert_eq!(
            reread.get(1).and_then(|item| item.timestamp),
            ItemTimestamp::from_timestamp(1_700_000_000, 0)
        );

        Ok(())
    }

    #[test]
    fn last_and_retain() -> Result<(), error::Error> {
        assert!(History::default().last().is_none());

        let mut history = history(&["a", "x", "b", "x", "c", "x"])?;
        assert_eq!(
            history.last().map(|item| item.command_line.as_str()),
            Some("x")
        );

        // Keeping everything changes nothing.
        history.retain(|_| true);
        assert_eq!(
            command_lines(history.iter()),
            ["a", "x", "b", "x", "c", "x"]
        );

        // Removing some keeps the rest in order, with their IDs.
        history.retain(|item| item.command_line != "x");
        assert_eq!(command_lines(history.iter()), ["a", "b", "c"]);
        assert_eq!(
            history.iter().map(|item| item.id).collect::<Vec<_>>(),
            [0, 2, 4]
        );
        assert_eq!(
            history.get_by_id(4)?.map(|item| item.command_line.as_str()),
            Some("c")
        );

        history.retain(|item| item.command_line != "a");
        assert_eq!(command_lines(history.iter()), ["b", "c"]);
        history.retain(|_| false);
        assert!(history.is_empty());

        Ok(())
    }

    /// `HISTCONTROL` parses as bash reads it: `ignoreboth` is two values, and unknown values,
    /// empty ones, and other cases are ignored.
    #[test]
    fn history_control_parses_like_bash() {
        use HistoryControl::{EraseDups, IgnoreDups, IgnoreSpace};
        let parse = |value| {
            HistoryControl::parse_list(value)
                .into_iter()
                .collect::<Vec<_>>()
        };

        assert_eq!(parse("ignoredups"), [IgnoreDups]);
        assert_eq!(parse("ignorespace"), [IgnoreSpace]);
        assert_eq!(parse("erasedups"), [EraseDups]);
        assert_eq!(parse("ignoreboth"), [IgnoreSpace, IgnoreDups]);
        assert_eq!(parse("erasedups:ignorespace"), [IgnoreSpace, EraseDups]);
        assert_eq!(parse("bogus::ignoredups:"), [IgnoreDups]);
        assert_eq!(parse("IGNOREDUPS"), []);
        assert_eq!(parse(""), []);
    }

    /// Searches and iterators over history can be held across an await in a spawned task.
    #[test]
    fn searches_are_send_and_sync() -> Result<(), error::Error> {
        const fn assert_send_sync<T: Send + Sync>(_: &T) {}

        let history = history(&["a"])?;
        assert_send_sync(&history.iter());
        assert_send_sync(&history.search(Query::default())?);

        Ok(())
    }

    /// Like bash, an entry the file gives no timestamp gets the time it's read.
    #[test]
    fn import_stamps_entries_without_timestamps() -> Result<(), error::Error> {
        let before = Utc::now();
        let history = History::import(&b"#1700000000\nls\necho hi\n"[..])?;

        let timestamps: Vec<_> = history.iter().map(|item| item.timestamp).collect();
        assert_eq!(
            timestamps[0],
            ItemTimestamp::from_timestamp(1_700_000_000, 0)
        );
        assert!(timestamps[1].is_some_and(|ts| ts >= before));

        Ok(())
    }
}
