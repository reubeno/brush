//! Pane role definitions for identifying panes in the layout.

/// Semantic role of a pane - its identity regardless of layout position.
///
/// Some pane roles support multiple instances (e.g., Terminal), while others
/// are always singular (e.g., Environment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum PaneRole {
    /// Terminal output pane - supports multiple instances
    Terminal(u32),

    /// Completion suggestions pane - always singular
    Completion,

    /// Environment variables pane - always singular
    Environment,

    /// Command history pane - always singular
    History,

    /// Shell aliases pane - always singular
    Aliases,

    /// Function definitions pane - always singular
    Functions,

    /// Call stack pane - always singular
    CallStack,
}

impl PaneRole {
    /// Primary terminal pane (the main terminal)
    #[allow(dead_code)]
    pub const PRIMARY_TERMINAL: Self = Self::Terminal(0);

    /// Creates a new terminal pane role with the given instance ID
    #[allow(dead_code)]
    pub const fn terminal(instance: u32) -> Self {
        Self::Terminal(instance)
    }
}

impl std::fmt::Display for PaneRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal(0) => write!(f, "Terminal"),
            Self::Terminal(n) => write!(f, "Terminal {}", n),
            Self::Completion => write!(f, "Completion"),
            Self::Environment => write!(f, "Environment"),
            Self::History => write!(f, "History"),
            Self::Aliases => write!(f, "Aliases"),
            Self::Functions => write!(f, "Functions"),
            Self::CallStack => write!(f, "CallStack"),
        }
    }
}
