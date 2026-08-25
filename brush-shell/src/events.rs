//! Facilities for configuring event tracing in the brush shell.

use std::{collections::HashSet, fmt::Display};

use brush_core::Error;
use tracing_subscriber::{
    Layer, Registry, filter::Targets, layer::SubscriberExt, reload::Handle, util::SubscriberInitExt,
};

/// Type of event to trace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TraceEvent {
    /// Traces parsing and evaluation of arithmetic expressions.
    Arithmetic,
    /// Traces command execution.
    Commands,
    /// Traces command completion generation.
    Complete,
    /// Traces word expansion.
    Expand,
    /// Traces functions.
    Functions,
    /// Traces input controls.
    Input,
    /// Traces job management.
    Jobs,
    /// Traces the process of parsing tokens into an abstract syntax tree.
    Parse,
    /// Traces pattern matching.
    Pattern,
    /// Traces the process of tokenizing input text.
    Tokenize,
    /// Traces usage of unimplemented functionality.
    Unimplemented,
}

impl TraceEvent {
    /// Parses a trace event name (as accepted by `--debug`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "arithmetic" => Self::Arithmetic,
            "commands" => Self::Commands,
            "complete" => Self::Complete,
            "expand" => Self::Expand,
            "functions" => Self::Functions,
            "input" => Self::Input,
            "jobs" => Self::Jobs,
            "parse" => Self::Parse,
            "pattern" => Self::Pattern,
            "tokenize" => Self::Tokenize,
            "unimplemented" | "unimp" => Self::Unimplemented,
            _ => return None,
        })
    }

    /// Returns all trace event names, in declaration order.
    #[must_use]
    pub const fn names() -> &'static [&'static str] {
        &[
            "arithmetic",
            "commands",
            "complete",
            "expand",
            "functions",
            "input",
            "jobs",
            "parse",
            "pattern",
            "tokenize",
            "unimplemented",
        ]
    }
}

impl std::str::FromStr for TraceEvent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("invalid trace event: `{s}`"))
    }
}

impl Display for TraceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arithmetic => write!(f, "arithmetic"),
            Self::Commands => write!(f, "commands"),
            Self::Complete => write!(f, "complete"),
            Self::Expand => write!(f, "expand"),
            Self::Functions => write!(f, "functions"),
            Self::Input => write!(f, "input"),
            Self::Jobs => write!(f, "jobs"),
            Self::Parse => write!(f, "parse"),
            Self::Pattern => write!(f, "pattern"),
            Self::Tokenize => write!(f, "tokenize"),
            Self::Unimplemented => write!(f, "unimplemented"),
        }
    }
}

#[derive(Default)]
pub(crate) struct TraceEventConfig {
    enabled_debug_events: HashSet<TraceEvent>,
    disabled_events: HashSet<TraceEvent>,
    handle: Option<Handle<Targets, Registry>>,
}

impl TraceEventConfig {
    pub fn init(enabled_debug_events: &[TraceEvent], disabled_events: &[TraceEvent]) -> Self {
        let enabled_debug_events: HashSet<TraceEvent> =
            enabled_debug_events.iter().copied().collect();
        let disabled_events: HashSet<TraceEvent> = disabled_events.iter().copied().collect();

        let mut config = Self {
            enabled_debug_events,
            disabled_events,
            ..Default::default()
        };

        let filter = config.compose_filter();

        // Make the filter reloadable so that we can change the log level at runtime.
        let (reload_filter, handle) = tracing_subscriber::reload::Layer::new(filter);

        let layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .without_time()
            .with_target(false)
            .with_filter(reload_filter);

        if tracing_subscriber::registry()
            .with(layer)
            .try_init()
            .is_ok()
        {
            config.handle = Some(handle);
        } else {
            // Something went wrong; proceed on anyway but complain audibly.
            eprintln!("warning: failed to initialize tracing.");
        }

        config
    }

    fn compose_filter(&self) -> tracing_subscriber::filter::Targets {
        let mut filter = tracing_subscriber::filter::Targets::new()
            .with_default(tracing_subscriber::filter::LevelFilter::INFO);

        for event in &self.enabled_debug_events {
            let targets = Self::event_to_tracing_targets(event);
            filter = filter.with_targets(
                targets
                    .into_iter()
                    .map(|target| (target, tracing::Level::DEBUG)),
            );
        }

        for event in &self.disabled_events {
            let targets = Self::event_to_tracing_targets(event);
            filter = filter.with_targets(
                targets
                    .into_iter()
                    .map(|target| (target, tracing::level_filters::LevelFilter::OFF)),
            );
        }

        filter
    }

    const fn event_to_tracing_targets(event: &TraceEvent) -> [&str; 1] {
        match event {
            TraceEvent::Arithmetic => ["arithmetic"],
            TraceEvent::Commands => ["commands"],
            TraceEvent::Complete => ["completion"],
            TraceEvent::Expand => ["expansion"],
            TraceEvent::Functions => ["functions"],
            TraceEvent::Input => ["input"],
            TraceEvent::Jobs => ["jobs"],
            TraceEvent::Parse => ["parse"],
            TraceEvent::Pattern => ["pattern"],
            TraceEvent::Tokenize => ["tokenize"],
            TraceEvent::Unimplemented => ["unimplemented"],
        }
    }

    pub const fn get_enabled_events(&self) -> &HashSet<TraceEvent> {
        &self.enabled_debug_events
    }

    pub fn enable(&mut self, event: TraceEvent) -> Result<(), Error> {
        // Don't bother to reload config if nothing has changed.
        if !self.enabled_debug_events.insert(event) {
            return Ok(());
        }

        self.reload_filter()
    }

    pub fn disable(&mut self, event: TraceEvent) -> Result<(), Error> {
        // Don't bother to reload config if nothing has changed.
        if !self.enabled_debug_events.remove(&event) {
            return Ok(());
        }

        self.reload_filter()
    }

    fn reload_filter(&self) -> Result<(), Error> {
        if let Some(handle) = &self.handle {
            if handle.reload(self.compose_filter()).is_ok() {
                Ok(())
            } else {
                Err(brush_core::ErrorKind::Unimplemented("failed to enable tracing events").into())
            }
        } else {
            Err(brush_core::ErrorKind::Unimplemented("tracing not initialized").into())
        }
    }
}
