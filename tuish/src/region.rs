//! Region management for tuish.
//!
//! A Region represents a visual area containing one or more panes arranged as tabs.

/// Unique identifier for a region
pub type RegionId = usize;

/// Unique identifier for a pane
pub type PaneId = usize;

/// A region contains one or more panes and maintains which pane has focus within the region.
#[derive(Debug, Clone)]
pub struct Region {
    /// Unique identifier for this region
    #[allow(dead_code)]
    id: RegionId,
    /// List of pane IDs in this region (at least one)
    panes: Vec<PaneId>,
    /// Index of the selected (focused) pane within this region
    selected_pane_index: usize,
    /// Whether this region can be split (for future split implementation)
    #[allow(dead_code)]
    splittable: bool,
    /// Whether this region can be closed (for future close implementation)
    #[allow(dead_code)]
    closeable: bool,
}

impl Region {
    /// Creates a new region with the given panes.
    ///
    /// # Panics
    /// Panics if `panes` is empty.
    #[must_use]
    pub fn new(id: RegionId, panes: Vec<PaneId>, splittable: bool, closeable: bool) -> Self {
        assert!(!panes.is_empty(), "Region must have at least one pane");
        Self {
            id,
            panes,
            selected_pane_index: 0,
            splittable,
            closeable,
        }
    }

    /// Returns the region ID.
    #[must_use]
    #[allow(dead_code)]
    pub const fn id(&self) -> RegionId {
        self.id
    }

    /// Returns the list of pane IDs in this region.
    #[must_use]
    pub fn panes(&self) -> &[PaneId] {
        &self.panes
    }

    /// Returns the currently focused pane ID.
    #[must_use]
    pub fn focused_pane(&self) -> PaneId {
        self.panes[self.selected_pane_index]
    }

    /// Checks if this region is focusable (has at least one enabled pane).
    #[must_use]
    pub fn is_focusable<F>(&self, mut pane_lookup: F) -> bool
    where
        F: FnMut(PaneId) -> bool,
    {
        self.panes.iter().any(|&pane_id| pane_lookup(pane_id))
    }

    /// Returns whether this region can be split (for future split implementation).
    #[must_use]
    #[allow(dead_code)]
    pub const fn splittable(&self) -> bool {
        self.splittable
    }

    /// Returns whether this region can be closed (for future close implementation).
    #[must_use]
    #[allow(dead_code)]
    pub const fn closeable(&self) -> bool {
        self.closeable
    }

    /// Selects the next pane in the region.
    pub fn select_next_pane(&mut self) {
        if !self.panes.is_empty() {
            self.selected_pane_index = (self.selected_pane_index + 1) % self.panes.len();
        }
    }

    /// Selects the previous pane in the region.
    pub fn select_prev_pane(&mut self) {
        if !self.panes.is_empty() {
            self.selected_pane_index = if self.selected_pane_index == 0 {
                self.panes.len() - 1
            } else {
                self.selected_pane_index - 1
            };
        }
    }

    /// Selects a specific pane by ID.
    ///
    /// Returns `true` if the pane was found and selected, `false` otherwise.
    pub fn select_pane(&mut self, pane_id: PaneId) -> bool {
        if let Some(index) = self.panes.iter().position(|&id| id == pane_id) {
            self.selected_pane_index = index;
            true
        } else {
            false
        }
    }

    /// Adds a pane to this region.
    pub fn add_pane(&mut self, pane_id: PaneId) {
        self.panes.push(pane_id);
    }

    /// Removes a pane from this region.
    ///
    /// Returns `true` if the pane was removed, `false` if not found.
    pub fn remove_pane(&mut self, pane_id: PaneId) -> bool {
        if let Some(idx) = self.panes.iter().position(|&id| id == pane_id) {
            self.panes.remove(idx);
            // Adjust selected index if needed
            if self.selected_pane_index >= self.panes.len() && !self.panes.is_empty() {
                self.selected_pane_index = self.panes.len() - 1;
            }
            true
        } else {
            false
        }
    }
}
