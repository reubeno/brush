use std::{collections::HashSet, fmt::Display};

use brush_core::Error;
use tracing_subscriber::{
    filter::Targets, layer::SubscriberExt, reload::Handle, util::SubscriberInitExt, Layer, Registry,
};

/// Type of event to trace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, clap::ValueEnum)]
pub enum TraceEvent {
    /// Traces parsing and evaluation of arithmetic expressions.
    #[clap(name = "arithmetic")]
    Arithmetic,
    /// Traces command execution.
    #[clap(name = "commands")]
    Commands,
    /// Traces command completion generation.
    #[clap(name = "complete")]
    Complete,
    /// Traces word expansion.
    #[clap(name = "expand")]
    Expand,
    /// Traces functions.
    #[clap(name = "functions")]
    Functions,
    /// Traces job management.
    #[clap(name = "jobs")]
    Jobs,
    /// Traces the process of parsing tokens into an abstract syntax tree.
    #[clap(name = "parse")]
    Parse,
    /// Traces pattern matching.
    #[clap(name = "pattern")]
    Pattern,
    /// Traces the process of tokenizing input text.
    #[clap(name = "tokenize")]
    Tokenize,
    /// Traces usage of unimplemented functionality.
    #[clap(name = "unimplemented", alias = "unimp")]
    Unimplemented,
}

impl Display for TraceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceEvent::Arithmetic => write!(f, "arithmetic"),
            TraceEvent::Commands => write!(f, "commands"),
            TraceEvent::Complete => write!(f, "complete"),
            TraceEvent::Expand => write!(f, "expand"),
            TraceEvent::Functions => write!(f, "functions"),
            TraceEvent::Jobs => write!(f, "jobs"),
            TraceEvent::Parse => write!(f, "parse"),
            TraceEvent::Pattern => write!(f, "pattern"),
            TraceEvent::Tokenize => write!(f, "tokenize"),
            TraceEvent::Unimplemented => write!(f, "unimplemented"),
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
    pub fn init(
        enabled_debug_events: &[TraceEvent],
        disabled_events: &[TraceEvent],
    ) -> TraceEventConfig {
        let enabled_debug_events: HashSet<TraceEvent> =
            enabled_debug_events.iter().copied().collect();
        let disabled_events: HashSet<TraceEvent> = disabled_events.iter().copied().collect();

        let mut config = TraceEventConfig {
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

    fn event_to_tracing_targets(event: &TraceEvent) -> Vec<&str> {
        match event {
            TraceEvent::Arithmetic => vec!["arithmetic"],
            TraceEvent::Commands => vec!["commands"],
            TraceEvent::Complete => vec!["completion"],
            TraceEvent::Expand => vec!["expansion"],
            TraceEvent::Functions => vec!["functions"],
            TraceEvent::Jobs => vec!["jobs"],
            TraceEvent::Parse => vec!["parse"],
            TraceEvent::Pattern => vec!["pattern"],
            TraceEvent::Tokenize => vec!["tokenize"],
            TraceEvent::Unimplemented => vec!["unimplemented"],
        }
    }

    pub fn get_enabled_events(&self) -> &HashSet<TraceEvent> {
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

    fn reload_filter(&mut self) -> Result<(), Error> {
        if let Some(handle) = &self.handle {
            if handle.reload(self.compose_filter()).is_ok() {
                Ok(())
            } else {
                Err(brush_core::Error::Unimplemented(
                    "failed to enable tracing events",
                ))
            }
        } else {
            Err(brush_core::Error::Unimplemented("tracing not initialized"))
        }
    }
}
