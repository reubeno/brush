//! Layout system for spatial arrangement of regions.
//!
//! The layout tree defines WHERE regions are rendered, not WHAT they contain.
//! It stores only RegionIds and handles splits and spatial calculations.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::region::RegionId;

/// Unique identifier for a layout node in the tree.
pub type LayoutId = usize;

/// A layout tree node representing spatial arrangements.
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// A region (rendered as tabs if it has multiple panes)
    Region {
        id: LayoutId,
        region_id: RegionId,
    },
    /// Horizontal split (left | right)
    #[allow(dead_code)]
    HSplit {
        id: LayoutId,
        left: Box<LayoutNode>,
        right: Box<LayoutNode>,
        split_percent: u16,
    },
    /// Vertical split (top / bottom)
    VSplit {
        id: LayoutId,
        top: Box<LayoutNode>,
        bottom: Box<LayoutNode>,
        split_percent: u16,
    },
}

impl LayoutNode {
    /// Returns the ID of this layout node.
    #[must_use]
    pub const fn id(&self) -> LayoutId {
        match self {
            Self::Region { id, .. } | Self::HSplit { id, .. } | Self::VSplit { id, .. } => *id,
        }
    }

    /// Finds a mutable reference to a node by ID.
    #[allow(dead_code)]
    pub fn find_node_mut(&mut self, target_id: LayoutId) -> Option<&mut Self> {
        if self.id() == target_id {
            return Some(self);
        }

        match self {
            Self::Region { .. } => None,
            Self::HSplit { left, right, .. } => {
                left.find_node_mut(target_id)
                    .or_else(|| right.find_node_mut(target_id))
            }
            Self::VSplit { top, bottom, .. } => {
                top.find_node_mut(target_id)
                    .or_else(|| bottom.find_node_mut(target_id))
            }
        }
    }

    /// Collects all region IDs from this layout tree.
    pub fn collect_region_ids(&self, result: &mut Vec<RegionId>) {
        match self {
            Self::Region { region_id, .. } => {
                result.push(*region_id);
            }
            Self::HSplit { left, right, .. } => {
                left.collect_region_ids(result);
                right.collect_region_ids(result);
            }
            Self::VSplit { top, bottom, .. } => {
                top.collect_region_ids(result);
                bottom.collect_region_ids(result);
            }
        }
    }

    /// Renders this layout node, returning region IDs with their rectangles.
    ///
    /// Returns a vector of (RegionId, Rect) tuples.
    pub fn render(&self, area: Rect) -> Vec<(RegionId, Rect)> {
        match self {
            Self::Region { region_id, .. } => {
                vec![(*region_id, area)]
            }
            Self::HSplit {
                left,
                right,
                split_percent,
                ..
            } => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(*split_percent),
                        Constraint::Percentage(100 - split_percent),
                    ])
                    .split(area);

                let mut result = left.render(chunks[0]);
                result.extend(right.render(chunks[1]));
                result
            }
            Self::VSplit {
                top,
                bottom,
                split_percent,
                ..
            } => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(*split_percent),
                        Constraint::Percentage(100 - split_percent),
                    ])
                    .split(area);

                let mut result = top.render(chunks[0]);
                result.extend(bottom.render(chunks[1]));
                result
            }
        }
    }
}

/// Manages the layout tree and focused region.
pub struct LayoutManager {
    root: LayoutNode,
    focused_region_id: Option<RegionId>,
    next_layout_id: LayoutId,
}

impl LayoutManager {
    /// Creates a new layout manager with the given root node.
    #[must_use]
    pub fn new(root: LayoutNode, initial_focused_region: RegionId) -> Self {
        let next_layout_id = root.id() + 1;
        Self {
            root,
            focused_region_id: Some(initial_focused_region),
            next_layout_id,
        }
    }

    /// Returns the currently focused region ID.
    #[must_use]
    pub const fn focused_region_id(&self) -> Option<RegionId> {
        self.focused_region_id
    }

    /// Sets the focused region ID.
    pub fn set_focused_region(&mut self, region_id: RegionId) {
        self.focused_region_id = Some(region_id);
    }

    /// Gets all region IDs in the layout.
    #[must_use]
    pub fn get_all_region_ids(&self) -> Vec<RegionId> {
        let mut result = Vec::new();
        self.root.collect_region_ids(&mut result);
        result
    }

    /// Focuses the next region in the layout.
    pub fn focus_next_region(&mut self) {
        let regions = self.get_all_region_ids();
        if regions.is_empty() {
            return;
        }

        if let Some(current) = self.focused_region_id {
            if let Some(idx) = regions.iter().position(|&r| r == current) {
                let next_idx = (idx + 1) % regions.len();
                self.focused_region_id = Some(regions[next_idx]);
                return;
            }
        }

        // No focus or not found - focus first
        self.focused_region_id = Some(regions[0]);
    }

    /// Focuses the previous region in the layout.
    pub fn focus_prev_region(&mut self) {
        let regions = self.get_all_region_ids();
        if regions.is_empty() {
            return;
        }

        if let Some(current) = self.focused_region_id {
            if let Some(idx) = regions.iter().position(|&r| r == current) {
                let prev_idx = if idx == 0 {
                    regions.len() - 1
                } else {
                    idx - 1
                };
                self.focused_region_id = Some(regions[prev_idx]);
                return;
            }
        }

        // No focus or not found - focus last
        self.focused_region_id = Some(regions[regions.len() - 1]);
    }

    /// Renders the layout, returning region IDs with their rectangles.
    #[must_use]
    pub fn render(&self, area: Rect) -> Vec<(RegionId, Rect)> {
        self.root.render(area)
    }



    /// Splits the current focused region vertically (left | right).
    ///
    /// Creates an HSplit with the current region on the left and a new region on the right.
    pub fn split_vertical(&mut self, new_region_id: RegionId) -> bool {
        let Some(focused_id) = self.focused_region_id else {
            return false;
        };

        // We need to recursively find and replace the region node
        let success = Self::split_vertical_recursive(&mut self.root, focused_id, new_region_id, &mut self.next_layout_id);
        
        if success {
            self.focused_region_id = Some(new_region_id);
        }
        
        success
    }

    fn split_vertical_recursive(node: &mut LayoutNode, target_region_id: RegionId, new_region_id: RegionId, next_id: &mut LayoutId) -> bool {
        match node {
            LayoutNode::Region { region_id, id } if *region_id == target_region_id => {
                // Found the target - replace it with an HSplit
                let current_region_id = *region_id;
                let new_left_id = *next_id;
                *next_id += 1;
                let new_right_id = *next_id;
                *next_id += 1;
                let new_split_id = *next_id;
                *next_id += 1;

                *node = LayoutNode::HSplit {
                    id: new_split_id,
                    left: Box::new(LayoutNode::Region {
                        id: new_left_id,
                        region_id: current_region_id,
                    }),
                    right: Box::new(LayoutNode::Region {
                        id: new_right_id,
                        region_id: new_region_id,
                    }),
                    split_percent: 50,
                };
                true
            }
            LayoutNode::HSplit { left, right, .. } => {
                Self::split_vertical_recursive(left, target_region_id, new_region_id, next_id) ||
                Self::split_vertical_recursive(right, target_region_id, new_region_id, next_id)
            }
            LayoutNode::VSplit { top, bottom, .. } => {
                Self::split_vertical_recursive(top, target_region_id, new_region_id, next_id) ||
                Self::split_vertical_recursive(bottom, target_region_id, new_region_id, next_id)
            }
            _ => false,
        }
    }

    /// Splits the current focused region horizontally (top / bottom).
    ///
    /// Creates a VSplit with the current region on top and a new region on the bottom.
    pub fn split_horizontal(&mut self, new_region_id: RegionId) -> bool {
        let Some(focused_id) = self.focused_region_id else {
            return false;
        };

        // We need to recursively find and replace the region node
        let success = Self::split_horizontal_recursive(&mut self.root, focused_id, new_region_id, &mut self.next_layout_id);
        
        if success {
            self.focused_region_id = Some(new_region_id);
        }
        
        success
    }

    fn split_horizontal_recursive(node: &mut LayoutNode, target_region_id: RegionId, new_region_id: RegionId, next_id: &mut LayoutId) -> bool {
        match node {
            LayoutNode::Region { region_id, id } if *region_id == target_region_id => {
                // Found the target - replace it with a VSplit
                let current_region_id = *region_id;
                let new_top_id = *next_id;
                *next_id += 1;
                let new_bottom_id = *next_id;
                *next_id += 1;
                let new_split_id = *next_id;
                *next_id += 1;

                *node = LayoutNode::VSplit {
                    id: new_split_id,
                    top: Box::new(LayoutNode::Region {
                        id: new_top_id,
                        region_id: current_region_id,
                    }),
                    bottom: Box::new(LayoutNode::Region {
                        id: new_bottom_id,
                        region_id: new_region_id,
                    }),
                    split_percent: 50,
                };
                true
            }
            LayoutNode::HSplit { left, right, .. } => {
                Self::split_horizontal_recursive(left, target_region_id, new_region_id, next_id) ||
                Self::split_horizontal_recursive(right, target_region_id, new_region_id, next_id)
            }
            LayoutNode::VSplit { top, bottom, .. } => {
                Self::split_horizontal_recursive(top, target_region_id, new_region_id, next_id) ||
                Self::split_horizontal_recursive(bottom, target_region_id, new_region_id, next_id)
            }
            _ => false,
        }
    }

    /// Removes a region from the layout by collapsing its parent split.
    ///
    /// When a region becomes empty, we need to remove it and replace its parent
    /// split with the sibling node. Returns true if the region was found and removed.
    pub fn remove_region(&mut self, region_id: RegionId) -> bool {
        let removed = Self::remove_region_recursive(&mut self.root, region_id);
        // If we removed the focused region, update focus
        if removed && self.focused_region_id == Some(region_id) {
            self.focus_next_region();
        }
        removed
    }

    fn remove_region_recursive(node: &mut LayoutNode, target_region_id: RegionId) -> bool {
        match node {
            LayoutNode::Region { region_id: _, .. } => {
                // Can't remove the root region node directly
                false
            }
            LayoutNode::HSplit { left, right, .. } => {
                // Check if left child is the target region
                if let LayoutNode::Region { region_id, .. } = **left {
                    if region_id == target_region_id {
                        // Replace this split with the right child
                        *node = (**right).clone();
                        return true;
                    }
                }
                
                // Check if right child is the target region
                if let LayoutNode::Region { region_id, .. } = **right {
                    if region_id == target_region_id {
                        // Replace this split with the left child
                        *node = (**left).clone();
                        return true;
                    }
                }
                
                // Recurse into children
                Self::remove_region_recursive(left, target_region_id)
                    || Self::remove_region_recursive(right, target_region_id)
            }
            LayoutNode::VSplit { top, bottom, .. } => {
                // Check if top child is the target region
                if let LayoutNode::Region { region_id, .. } = **top {
                    if region_id == target_region_id {
                        // Replace this split with the bottom child
                        *node = (**bottom).clone();
                        return true;
                    }
                }
                
                // Check if bottom child is the target region
                if let LayoutNode::Region { region_id, .. } = **bottom {
                    if region_id == target_region_id {
                        // Replace this split with the top child
                        *node = (**top).clone();
                        return true;
                    }
                }
                
                // Recurse into children
                Self::remove_region_recursive(top, target_region_id)
                    || Self::remove_region_recursive(bottom, target_region_id)
            }
        }
    }
}
