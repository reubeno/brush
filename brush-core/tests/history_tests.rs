//! Tests of history that depend on the platform: here, a device that's always full.

#![cfg(target_os = "linux")]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use brush_core::history::{History, Item};

/// A flush that fails leaves the items it was to write unsaved, so a later one retries them
/// rather than losing them (e.g. after a full disk).
#[test]
fn failed_flush_leaves_items_unsaved() -> Result<()> {
    let mut history = History::default();
    for line in ["a", "b", "c"] {
        history.add(Item::new(line))?;
    }

    assert!(history.flush("/dev/full", true, true, false).is_err());
    assert!(history.iter().all(|item| item.dirty));

    Ok(())
}
