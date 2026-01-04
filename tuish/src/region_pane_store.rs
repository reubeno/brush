//! Storage and management of regions and panes.
//!
//! This module provides the central store for all regions and panes in the application.
//! It is responsible solely for storing and providing access to these objects.

use std::collections::HashMap;

use crate::content_pane::ContentPane;
use crate::region::{PaneId, Region, RegionId};

/// Central store for all regions and panes.
///
/// This struct owns all Region and Pane objects and provides lookup by ID.
/// It does NOT handle focus, layout, or rendering - those are separate concerns.
pub struct RegionPaneStore {
    /// All regions indexed by ID
    regions: HashMap<RegionId, Region>,
    /// All panes indexed by ID
    panes: HashMap<PaneId, Box<dyn ContentPane>>,
    /// Next available region ID
    next_region_id: RegionId,
    /// Next available pane ID
    next_pane_id: PaneId,
}

impl RegionPaneStore {
    /// Creates a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
            panes: HashMap::new(),
            next_region_id: 0,
            next_pane_id: 0,
        }
    }

    /// Creates a new region with the given panes.
    ///
    /// Returns the ID of the newly created region.
    pub fn create_region(
        &mut self,
        panes: Vec<PaneId>,
        splittable: bool,
        closeable: bool,
    ) -> RegionId {
        let region_id = self.next_region_id;
        self.next_region_id += 1;

        let region = Region::new(region_id, panes, splittable, closeable);
        self.regions.insert(region_id, region);

        region_id
    }

    /// Adds a pane to the store.
    ///
    /// Returns the ID assigned to the pane.
    pub fn add_pane(&mut self, pane: Box<dyn ContentPane>) -> PaneId {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        self.panes.insert(pane_id, pane);

        pane_id
    }

    /// Gets a reference to a region by ID.
    #[must_use]
    pub fn get_region(&self, id: RegionId) -> Option<&Region> {
        self.regions.get(&id)
    }

    /// Gets a mutable reference to a region by ID.
    pub fn get_region_mut(&mut self, id: RegionId) -> Option<&mut Region> {
        self.regions.get_mut(&id)
    }

    /// Gets a reference to a pane by ID.
    #[must_use]
    pub fn get_pane(&self, id: PaneId) -> Option<&dyn ContentPane> {
        self.panes.get(&id).map(|b| &**b)
    }

    /// Gets a mutable reference to a pane by ID.
    pub fn get_pane_mut(&mut self, id: PaneId) -> Option<&mut Box<dyn ContentPane>> {
        self.panes.get_mut(&id)
    }

    /// Returns an iterator over all pane IDs.
    pub fn pane_ids(&self) -> impl Iterator<Item = PaneId> + '_ {
        self.panes.keys().copied()
    }

    /// Checks if a pane is enabled.
    ///
    /// Returns `false` if the pane doesn't exist.
    #[must_use]
    pub fn is_pane_enabled(&self, id: PaneId) -> bool {
        self.panes.get(&id).is_some_and(|pane| pane.is_enabled())
    }

    /// Checks if a region is focusable (has at least one enabled pane).
    ///
    /// Returns `false` if the region doesn't exist.
    #[must_use]
    pub fn is_region_focusable(&self, id: RegionId) -> bool {
        self.regions
            .get(&id)
            .is_some_and(|region| region.is_focusable(|pane_id| self.is_pane_enabled(pane_id)))
    }

    /// Gets the focused pane ID for a region.
    ///
    /// Returns `None` if the region doesn't exist.
    #[must_use]
    pub fn get_region_focused_pane(&self, region_id: RegionId) -> Option<PaneId> {
        self.regions.get(&region_id).map(|r| r.focused_pane())
    }
}

impl Default for RegionPaneStore {
    fn default() -> Self {
        Self::new()
    }
}
