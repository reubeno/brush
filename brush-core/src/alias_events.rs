//! Emits notifications when shell aliases change.

use serde::Serialize;
use std::sync::OnceLock;
use tokio::sync::mpsc::UnboundedSender;

/// Describes an alias-related event.
#[derive(Clone, Debug, Serialize)]
pub enum AliasEvent {
    /// An alias was added or updated.
    Set {
        /// Alias name.
        name: String,
        /// Alias expansion.
        value: String,
    },
    /// An alias was removed.
    Unset {
        /// Alias name.
        name: String,
    },
}

static ALIAS_EVENT_SENDER: OnceLock<UnboundedSender<AliasEvent>> = OnceLock::new();

/// Registers a sender that will receive [`AliasEvent`] notifications.
/// Subsequent calls have no effect once a sender has been registered.
pub fn set_alias_event_sender(sender: UnboundedSender<AliasEvent>) {
    let _ = ALIAS_EVENT_SENDER.set(sender);
}

/// Emits an [`AliasEvent`] to any registered subscribers.
pub fn emit(event: AliasEvent) {
    if let Some(sender) = ALIAS_EVENT_SENDER.get() {
        let _ = sender.send(event);
    }
}

