//! Layout system for flexible pane arrangements.
//!
//! Supports tabs, horizontal splits, and vertical splits in a tree structure.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::fmt;

/// Unique identifier for pane instances
pub type PaneId = usize;

/// Unique identifier for a layout node in the tree.
pub type LayoutId = usize;

/// A layout tree node representing pane arrangements.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Split rendering not yet fully integrated
pub enum LayoutNode {
    /// Tabbed region containing one or more panes
    Tabs {
        id: LayoutId,
        panes: Vec<PaneId>,
        selected: usize,
        /// Whether this region can be split
        splittable: bool,
        /// Whether this region can be closed
        closeable: bool,
    },
    /// Horizontal split (left | right)
    HSplit {
        id: LayoutId,
        left: Box<LayoutNode>,
        right: Box<LayoutNode>,
        /// Percentage for left pane (0-100)
        split_percent: u16,
    },
    /// Vertical split (top / bottom)
    VSplit {
        id: LayoutId,
        top: Box<LayoutNode>,
        bottom: Box<LayoutNode>,
        /// Percentage for top pane (0-100)
        split_percent: u16,
    },
}

#[allow(dead_code, clippy::missing_const_for_fn, clippy::doc_markdown)]
impl LayoutNode {
    /// Returns the ID of this node.
    pub const fn id(&self) -> LayoutId {
        match self {
            Self::Tabs { id, .. } | Self::HSplit { id, .. } | Self::VSplit { id, .. } => *id,
        }
    }

    /// Collects all visible panes (IDs) from this layout.
    ///
    /// For tabs, only includes the selected tab.
    pub fn visible_panes(&self) -> Vec<PaneId> {
        match self {
            Self::Tabs {
                panes,
                selected,
                splittable: _,
                closeable: _,
                ..
            } => {
                if *selected < panes.len() {
                    vec![panes[*selected]]
                } else {
                    vec![]
                }
            }
            Self::HSplit { left, right, .. } => {
                let mut result = left.visible_panes();
                result.extend(right.visible_panes());
                result
            }
            Self::VSplit { top, bottom, .. } => {
                let mut result = top.visible_panes();
                result.extend(bottom.visible_panes());
                result
            }
        }
    }

    /// Collects ALL panes (IDs) from this layout, including non-visible tabs.
    pub fn all_panes(&self) -> Vec<PaneId> {
        match self {
            Self::Tabs { panes, .. } => panes.clone(),
            Self::HSplit { left, right, .. } => {
                let mut result = left.all_panes();
                result.extend(right.all_panes());
                result
            }
            Self::VSplit { top, bottom, .. } => {
                let mut result = top.all_panes();
                result.extend(bottom.all_panes());
                result
            }
        }
    }

    /// Returns the LayoutId and PaneId of the currently focused pane, if any.
    ///
    /// For tabs, returns the selected pane. For splits, recursively finds first visible.
    pub fn focused_pane(&self) -> Option<(LayoutId, PaneId)> {
        match self {
            Self::Tabs {
                id,
                panes,
                selected,
                splittable: _,
                closeable: _,
            } => {
                if *selected < panes.len() {
                    Some((*id, panes[*selected]))
                } else {
                    None
                }
            }
            Self::HSplit { left, .. } => left.focused_pane(),
            Self::VSplit { top, .. } => top.focused_pane(),
        }
    }

    /// Returns the focused pane within the specified region.
    ///
    /// Searches for the region with the given ID and returns its focused pane.
    pub fn focused_pane_in_region(&self, focused_region_id: LayoutId) -> Option<(LayoutId, PaneId)> {
        match self {
            Self::Tabs {
                id,
                panes,
                selected,
                splittable: _,
                closeable: _,
            } => {
                if *id == focused_region_id && *selected < panes.len() {
                    Some((*id, panes[*selected]))
                } else {
                    None
                }
            }
            Self::HSplit { left, right, .. } => {
                left.focused_pane_in_region(focused_region_id)
                    .or_else(|| right.focused_pane_in_region(focused_region_id))
            }
            Self::VSplit { top, bottom, .. } => {
                top.focused_pane_in_region(focused_region_id)
                    .or_else(|| bottom.focused_pane_in_region(focused_region_id))
            }
        }
    }

    /// Renders this layout node into the given area, returning rectangles for each region.
    ///
    /// Returns a vector of `(LayoutId, Vec<PaneId>, usize, Rect)` tuples representing regions.
    /// Each tuple contains: region ID, all panes in region, selected tab index, and rectangle.
    pub fn render_layout(&self, area: Rect) -> Vec<(LayoutId, Vec<PaneId>, usize, Rect)> {
        match self {
            Self::Tabs {
                id,
                panes,
                selected,
                splittable: _,
                closeable: _,
            } => {
                vec![(*id, panes.clone(), *selected, area)]
            }
            Self::HSplit {
                id: _,
                left,
                right,
                split_percent,
            } => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(*split_percent),
                        Constraint::Percentage(100 - split_percent),
                    ])
                    .split(area);

                let mut result = left.render_layout(chunks[0]);
                result.extend(right.render_layout(chunks[1]));
                result
            }
            Self::VSplit {
                id: _,
                top,
                bottom,
                split_percent,
            } => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(*split_percent),
                        Constraint::Percentage(100 - split_percent),
                    ])
                    .split(area);

                let mut result = top.render_layout(chunks[0]);
                result.extend(bottom.render_layout(chunks[1]));
                result
            }
        }
    }

    /// Finds a node by its ID and returns a mutable reference.
    pub fn find_node_mut(&mut self, target_id: LayoutId) -> Option<&mut Self> {
        if self.id() == target_id {
            return Some(self);
        }

        match self {
            Self::Tabs { .. } => None,
            Self::HSplit { left, right, .. } => left
                .find_node_mut(target_id)
                .or_else(|| right.find_node_mut(target_id)),
            Self::VSplit { top, bottom, .. } => top
                .find_node_mut(target_id)
                .or_else(|| bottom.find_node_mut(target_id)),
        }
    }
}

impl fmt::Display for LayoutNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tabs {
                panes, selected, ..
            } => {
                write!(f, "Tabs[")?;
                for (i, pane_id) in panes.iter().enumerate() {
                    if i == *selected {
                        write!(f, "*{pane_id}")?;
                    } else {
                        write!(f, "{pane_id}")?;
                    }
                    if i < panes.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
            Self::HSplit { left, right, .. } => write!(f, "({left} | {right})"),
            Self::VSplit { top, bottom, .. } => write!(f, "({top} / {bottom})"),
        }
    }
}

/// Manages layout state and ID generation.
pub struct LayoutManager {
    #[allow(dead_code)]
    next_id: LayoutId,
    root: LayoutNode,
    focused_node_id: Option<LayoutId>,
}

#[allow(clippy::missing_const_for_fn, clippy::doc_markdown, dead_code)]
impl LayoutManager {
    /// Creates a new layout manager with the given root node.
    pub fn new(root: LayoutNode) -> Self {
        let focused_node_id = Some(root.id());
        Self {
            next_id: root.id() + 1,
            root,
            focused_node_id,
        }
    }

    /// Creates a default tabbed layout from a list of pane IDs.
    pub fn new_tabs(panes: Vec<PaneId>, selected: usize) -> Self {
        let root = LayoutNode::Tabs {
            id: 0,
            panes,
            selected,
            splittable: true,
            closeable: true,
        };
        Self::new(root)
    }

    /// Returns a reference to the root layout node.
    #[allow(dead_code)]
    pub const fn root(&self) -> &LayoutNode {
        &self.root
    }

    /// Returns the currently focused node ID.
    #[allow(dead_code)]
    pub const fn focused_node_id(&self) -> Option<LayoutId> {
        self.focused_node_id
    }

    /// Returns the currently focused pane ID, if any.
    pub fn focused_pane(&self) -> Option<PaneId> {
        self.focused_node_id
            .and_then(|focused_id| self.root.focused_pane_in_region(focused_id))
            .map(|(_, pane_id)| pane_id)
    }

    /// Generates a new unique layout ID.
    #[allow(dead_code)]
    fn next_id(&mut self) -> LayoutId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Splits the focused region vertically (side by side).
    ///
    /// Splits OUT the currently selected pane from the focused region:
    /// - If the region has only 1 pane, creates a new pane (given as parameter)
    /// - If the region has multiple panes, the selected pane moves to new region on right
    pub fn split_vertical(&mut self, new_pane_id: PaneId) -> bool {
        let Some(focused_id) = self.focused_node_id else {
            return false;
        };

        // Generate IDs before borrowing
        let new_region_id = self.next_id();
        let split_id = self.next_id();

        let Some(node) = self.root.find_node_mut(focused_id) else {
            return false;
        };

        // Extract the pane to split out (or use new_pane_id if only one pane)
        let split_out_pane = if let LayoutNode::Tabs { panes, selected, .. } = node {
            if panes.len() > 1 {
                // Multiple panes: remove and split out the selected one
                let pane_id = panes.remove(*selected);
                // Adjust selected index if needed
                if *selected >= panes.len() && !panes.is_empty() {
                    *selected = panes.len() - 1;
                }
                pane_id
            } else {
                // Single pane: use the new pane ID (create new pane)
                new_pane_id
            }
        } else {
            return false;
        };

        // Take ownership of the current node
        let old_node = std::mem::replace(
            node,
            LayoutNode::Tabs {
                id: 0,
                panes: vec![],
                selected: 0,
                splittable: true,
                closeable: true,
            },
        );

        // Replace with HSplit
        *node = LayoutNode::HSplit {
            id: split_id,
            left: Box::new(old_node),
            right: Box::new(LayoutNode::Tabs {
                id: new_region_id,
                panes: vec![split_out_pane],
                selected: 0,
                splittable: true,
                closeable: true,
            }),
            split_percent: 50,
        };

        // Focus the new region (with the split-out pane)
        self.focused_node_id = Some(new_region_id);
        true
    }

    /// Splits the focused region horizontally (top and bottom).
    ///
    /// Splits OUT the currently selected pane from the focused region:
    /// - If the region has only 1 pane, creates a new pane (given as parameter)
    /// - If the region has multiple panes, the selected pane moves to new region on bottom
    pub fn split_horizontal(&mut self, new_pane_id: PaneId) -> bool {
        let Some(focused_id) = self.focused_node_id else {
            return false;
        };

        // Generate IDs before borrowing
        let new_region_id = self.next_id();
        let split_id = self.next_id();

        let Some(node) = self.root.find_node_mut(focused_id) else {
            return false;
        };

        // Extract the pane to split out (or use new_pane_id if only one pane)
        let split_out_pane = if let LayoutNode::Tabs { panes, selected, .. } = node {
            if panes.len() > 1 {
                // Multiple panes: remove and split out the selected one
                let pane_id = panes.remove(*selected);
                // Adjust selected index if needed
                if *selected >= panes.len() && !panes.is_empty() {
                    *selected = panes.len() - 1;
                }
                pane_id
            } else {
                // Single pane: use the new pane ID (create new pane)
                new_pane_id
            }
        } else {
            return false;
        };

        // Take ownership of the current node
        let old_node = std::mem::replace(
            node,
            LayoutNode::Tabs {
                id: 0,
                panes: vec![],
                selected: 0,
                splittable: true,
                closeable: true,
            },
        );

        // Replace with VSplit
        *node = LayoutNode::VSplit {
            id: split_id,
            top: Box::new(old_node),
            bottom: Box::new(LayoutNode::Tabs {
                id: new_region_id,
                panes: vec![split_out_pane],
                selected: 0,
                splittable: true,
                closeable: true,
            }),
            split_percent: 50,
        };

        // Focus the new region (with the split-out pane)
        self.focused_node_id = Some(new_region_id);
        true
    }

    /// Sets the focused node by ID.
    #[allow(dead_code)]
    pub fn set_focused_node(&mut self, id: LayoutId) {
        self.focused_node_id = Some(id);
    }

    /// Renders the layout into rectangles for each region.
    ///
    /// Returns: (region_id, pane_ids, selected_index, rect) for each region.
    pub fn render_layout(&self, area: Rect) -> Vec<(LayoutId, Vec<PaneId>, usize, Rect)> {
        self.root.render_layout(area)
    }

    /// Adds a pane to the specified region (must be a Tabs node).
    pub fn add_pane_to_region(&mut self, region_id: LayoutId, pane_id: PaneId) -> bool {
        let Some(node) = self.root.find_node_mut(region_id) else {
            return false;
        };

        if let LayoutNode::Tabs { panes, .. } = node {
            panes.push(pane_id);
            true
        } else {
            false
        }
    }

    /// Gets all region IDs (Tabs nodes) in the layout tree.
    pub fn get_all_regions(&self) -> Vec<LayoutId> {
        fn collect_regions(node: &LayoutNode, result: &mut Vec<LayoutId>) {
            match node {
                LayoutNode::Tabs { id, .. } => {
                    result.push(*id);
                }
                LayoutNode::HSplit { left, right, .. } => {
                    collect_regions(left, result);
                    collect_regions(right, result);
                }
                LayoutNode::VSplit { top, bottom, .. } => {
                    collect_regions(top, result);
                    collect_regions(bottom, result);
                }
            }
        }

        let mut regions = Vec::new();
        collect_regions(&self.root, &mut regions);
        regions
    }

    /// Cycles focus to the next region.
    pub fn focus_next_region(&mut self) {
        let regions = self.get_all_regions();
        if regions.is_empty() {
            return;
        }

        if let Some(current_idx) = self
            .focused_node_id
            .and_then(|id| regions.iter().position(|&r| r == id))
        {
            let next_idx = (current_idx + 1) % regions.len();
            self.focused_node_id = Some(regions[next_idx]);
        } else {
            // No focus set, focus first region
            self.focused_node_id = Some(regions[0]);
        }
    }

    /// Cycles focus to the previous region.
    pub fn focus_prev_region(&mut self) {
        let regions = self.get_all_regions();
        if regions.is_empty() {
            return;
        }

        if let Some(current_idx) = self
            .focused_node_id
            .and_then(|id| regions.iter().position(|&r| r == id))
        {
            let prev_idx = if current_idx == 0 {
                regions.len() - 1
            } else {
                current_idx - 1
            };
            self.focused_node_id = Some(regions[prev_idx]);
        } else {
            // No focus set, focus last region
            self.focused_node_id = Some(regions[regions.len() - 1]);
        }
    }

    /// Cycles to the next tab within the focused region.
    pub fn cycle_tabs_in_focused_region(&mut self, forward: bool) {
        let Some(focused_id) = self.focused_node_id else {
            return;
        };

        let Some(node) = self.root.find_node_mut(focused_id) else {
            return;
        };

        if let LayoutNode::Tabs {
            panes, selected, splittable: _, closeable: _, ..
        } = node
        {
            if panes.is_empty() {
                return;
            }

            if forward {
                *selected = (*selected + 1) % panes.len();
            } else {
                *selected = if *selected == 0 {
                    panes.len() - 1
                } else {
                    *selected - 1
                };
            }
        }
    }

    /// Focuses a specific pane, finding its containing region and selecting it.
    /// Returns true if the pane was found and focused.
    pub fn focus_pane(&mut self, target_pane_id: PaneId) -> bool {
        // Helper function to find the region containing the pane and its index
        fn find_pane_in_node(node: &LayoutNode, pane_id: PaneId) -> Option<(LayoutId, usize)> {
            match node {
                LayoutNode::Tabs {
                    id,
                    panes,
                    selected: _,
                    splittable: _,
                    closeable: _,
                } => panes
                    .iter()
                    .position(|&p| p == pane_id)
                    .map(|idx| (*id, idx)),
                LayoutNode::HSplit { left, right, .. } => {
                    find_pane_in_node(left, pane_id).or_else(|| find_pane_in_node(right, pane_id))
                }
                LayoutNode::VSplit { top, bottom, .. } => {
                    find_pane_in_node(top, pane_id).or_else(|| find_pane_in_node(bottom, pane_id))
                }
            }
        }

        // Find which region contains this pane
        if let Some((region_id, pane_idx)) = find_pane_in_node(&self.root, target_pane_id) {
            // Focus that region
            self.focused_node_id = Some(region_id);

            // Select the pane within that region
            if let Some(LayoutNode::Tabs { selected, .. }) = self.root.find_node_mut(region_id) {
                *selected = pane_idx;
            }

            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tabs_visible_panes() {
        let layout = LayoutNode::Tabs {
            id: 0,
            panes: vec![0, 1], // PaneIds 0 and 1
            selected: 0,
        };

        let visible = layout.visible_panes();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0], 0); // Selected pane ID
    }

    #[test]
    fn test_split_vertical() {
        let mut manager = LayoutManager::new_tabs(vec![0], 0); // Pane ID 0
        assert!(manager.split_vertical(1)); // Split with pane ID 1

        let visible = manager.root().visible_panes();
        assert_eq!(visible.len(), 2);
    }
}
