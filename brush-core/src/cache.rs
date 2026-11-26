//! Per-thread caches of parse results.
//!
//! Each thread keeps its own cache, so shells running on many threads at once
//! never wait on each other.

use std::cell::RefCell;
use std::hash::Hash;
use std::thread::LocalKey;

use cached::{Cached, LruCache};

/// A per-thread cache, declared with `thread_local!` and built by [`new`]. It is
/// `None` if the cache could not be built, and every parse then runs uncached.
pub(crate) type Cache<K, V> = RefCell<Option<LruCache<K, V>>>;

/// Builds a cache of 64 entries.
pub(crate) fn new<K: Hash + Eq + Clone, V: Clone>() -> Cache<K, V> {
    RefCell::new(LruCache::builder().max_size(64).build().ok())
}

/// Returns the cached value for `key`, or parses it and caches the value if the
/// parse succeeds.
pub(crate) fn get_or_parse<K, V, E>(
    cache: &'static LocalKey<Cache<K, V>>,
    key: K,
    parse: impl FnOnce() -> Result<V, E>,
) -> Result<V, E>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    let hit = cache.with_borrow_mut(|cache| cache.as_mut()?.cache_get(&key).cloned());
    if let Some(value) = hit {
        return Ok(value);
    }
    let value = parse()?;
    cache.with_borrow_mut(|cache| {
        if let Some(cache) = cache {
            cache.cache_set(key, value.clone());
        }
    });
    Ok(value)
}
