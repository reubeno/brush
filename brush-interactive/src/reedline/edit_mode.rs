//! The bindings the editor holds, and how a keystroke resolves against them.
//!
//! [`UpdatableBindings`] is the one [`interfaces::KeyBindings`] implementation `bind` talks
//! to, and at the same time reedline's [`EditMode`](reedline::EditMode). It owns what a key
//! sequence is bound to and turns input into editor events; the stream that input arrives on
//! is [`super::pending`], the bytes-to-key conversions are [`super::keys`], and the
//! translation to and from reedline's own events is [`super::events`].

use brush_core::{
    interfaces::{self, KeyAction, KeyMacro, KeySequence},
    trace_categories,
};
use radix_trie::Trie;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use tokio::sync::Mutex;

use super::events::{
    DeferredAction, HostCommand, KeyError, combine, flatten, translate_action_to_reedline_event,
    translate_reedline_event_to_action,
};
use super::keys::{MAX_KEY_LEN, canonical, input_sequence_for, key_sequence_for, lift_key};
use super::pending::{DeferredReplay, Pending, PendingInput};

pub(crate) struct MutableEditMode {
    inner: Arc<Mutex<UpdatableBindings>>,
}

impl MutableEditMode {
    pub fn new(bindings: reedline::Keybindings) -> Self {
        Self {
            inner: Arc::new(Mutex::new(UpdatableBindings::new(bindings))),
        }
    }

    pub fn bindings(&self) -> Arc<Mutex<UpdatableBindings>> {
        self.inner.clone()
    }
}

impl reedline::EditMode for MutableEditMode {
    fn parse_event(&mut self, event: reedline::ReedlineRawEvent) -> reedline::ReedlineEvent {
        let mut inner = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.inner.lock())
        });

        inner.parse_event(event)
    }

    fn edit_mode(&self) -> reedline::PromptEditMode {
        let inner = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.inner.lock())
        });

        inner.edit_mode()
    }
}

/// Key bindings that can be changed while the editor is running.
///
/// Key sequences are bytes throughout, in canonical form (see [`canonical`]). A single key
/// bound to an action lives in reedline's own map, which dispatches it; everything else
/// (macros, and actions on longer sequences) lives in one byte trie. Macros are stored only
/// as the bytes they replay and resolved when their key is pressed, against the bindings as
/// they are at that moment; the reedline map never holds a macro.
pub(crate) struct UpdatableBindings {
    /// Base bindings, as reedline sees them. Never contain macro-owned keys.
    bindings: reedline::Keybindings,
    edit_mode: Box<dyn reedline::EditMode>,
    /// Every binding reedline does not dispatch itself, keyed by canonical bytes. One entry
    /// per key, so a key holds an action or a macro, never both.
    bindings_by_bytes: Trie<Vec<u8>, Binding>,
    /// Length of the longest key in `bindings_by_bytes`, kept current by
    /// [`Self::insert_byte_binding`] and [`Self::remove_byte_binding`] so that a keystroke
    /// does not walk the trie to size its matching window.
    longest_sequence: usize,
    /// Unconsumed terminal keys and macro bytes, in one expansion stream.
    pending: Pending,
    /// Macro bytes each keystroke may replay: [`MAX_MACRO_REPLAY_BYTES`], lowered by tests
    /// that drive a budget to exhaustion.
    max_replay: usize,
    /// Macro bytes the keystroke being resolved may still replay.
    budget: usize,
    /// Records referenced by host events. Only the record reedline returns is claimed; the
    /// rest are discarded at the read boundary.
    deferred: Vec<Option<Deferred>>,
}

/// What a key sequence in [`UpdatableBindings::bindings_by_bytes`] is bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Binding {
    /// A shell command or readline function.
    Action(KeyAction),
    /// A readline macro: bytes replayed through the active keymap.
    Macro(KeyMacro),
}

/// How many macro bytes one keystroke may replay, counting those reached across a bound
/// command. Macros nest and chain, so only a budget bounds the work of a self-referential
/// or mutually recursive binding, and resolution costs time per byte replayed, so the
/// budget is in bytes: a wide body spends it as surely as a deep chain does. Far above what
/// any real chain needs (atuin, fzf and zoxide replay under a hundred bytes each), and
/// small enough that a runaway binding stalls a keystroke for a fraction of a second, not
/// minutes. readline bounds nesting depth (16) instead, which leaves width unbounded.
pub(super) const MAX_MACRO_REPLAY_BYTES: usize = 16 * 1024;

/// A point at which macro resolution stopped: what to do, and what was still pending.
/// Only its index travels through reedline's string-only host interface, as
/// [`HostCommand::Deferred`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Deferred {
    pub action: DeferredAction,
    /// The input still held behind the action, if any.
    pub replay: Option<DeferredReplay>,
}

impl UpdatableBindings {
    pub fn new(bindings: reedline::Keybindings) -> Self {
        let edit_mode = Self::rebuild_edit_mode(&bindings);

        Self {
            bindings,
            edit_mode,
            bindings_by_bytes: Trie::new(),
            longest_sequence: 0,
            pending: Pending::default(),
            max_replay: MAX_MACRO_REPLAY_BYTES,
            budget: MAX_MACRO_REPLAY_BYTES,
            deferred: Vec::new(),
        }
    }

    pub fn update(&mut self, f: impl Fn(&mut reedline::Keybindings)) {
        f(&mut self.bindings);
        self.edit_mode = Self::rebuild_edit_mode(&self.bindings);
    }

    fn rebuild_edit_mode(bindings: &reedline::Keybindings) -> Box<dyn reedline::EditMode> {
        Box::new(reedline::Emacs::new(bindings.clone()))
    }

    /// Ends a read: claims the record reedline returned, if it named one, and drops
    /// everything else the read was holding (keys held for a sequence that can no longer
    /// finish, and records nothing returned). Returns `None` for a `claim` no record is held
    /// for, which indicates a bookkeeping bug rather than user input.
    pub fn end_read(&mut self, claim: Option<usize>) -> Option<Deferred> {
        let claimed = claim.and_then(|id| self.deferred.get_mut(id).and_then(Option::take));
        self.pending.clear();
        self.deferred.clear();
        self.budget = self.max_replay;
        claimed
    }

    /// Lowers the per-keystroke replay budget, so a test can drive one to exhaustion.
    #[cfg(test)]
    pub const fn set_max_replay(&mut self, max_replay: usize) {
        self.max_replay = max_replay;
        self.budget = max_replay;
    }

    /// Resumes the pending input against the bindings as they are after the host command,
    /// with what was left of its keystroke's replay budget.
    pub fn resolve_replay(&mut self, replay: DeferredReplay) -> reedline::ReedlineEvent {
        self.pending.append(replay.pending);
        self.budget = replay.budget;
        self.resolve_pending()
    }
}

impl reedline::EditMode for UpdatableBindings {
    fn parse_event(&mut self, event: reedline::ReedlineRawEvent) -> reedline::ReedlineEvent {
        self.dispatch(event.into())
    }

    fn edit_mode(&self) -> reedline::PromptEditMode {
        self.edit_mode.edit_mode()
    }
}

impl interfaces::KeyBindings for UpdatableBindings {
    fn get_current(&self) -> BTreeMap<KeySequence, KeyAction> {
        use radix_trie::TrieCommon;

        // Two reedline keys can spell the same sequence: the terminal reports `A` as
        // Shift+`A`, and a binding on Shift+`a` would spell `a` too. reedline's map has no
        // defined iteration order, so pick by a rule instead of by whichever came last:
        // fewest modifiers wins, then the lowest bits, and `bind -p` stays stable.
        let mut ranked: BTreeMap<KeySequence, (u32, u8, KeyAction)> = BTreeMap::new();
        for (key_combo, event) in self.bindings.get_keybindings() {
            if let Some(action) = translate_reedline_event_to_action(event)
                && let Some(seq) = input_sequence_for(key_combo.modifier, key_combo.key_code)
            {
                let bits = key_combo.modifier.bits();
                let rank = (bits.count_ones(), bits);
                if ranked
                    .get(&seq)
                    .is_none_or(|(count, bits, _)| rank < (*count, *bits))
                {
                    ranked.insert(seq, (rank.0, rank.1, action));
                }
            }
        }

        let mut results: BTreeMap<_, _> = ranked
            .into_iter()
            .map(|(seq, (_, _, action))| (seq, action))
            .collect();

        for (bytes, binding) in self.bindings_by_bytes.iter() {
            if let Binding::Action(action) = binding {
                results.insert(KeySequence::from(bytes.clone()), action.clone());
            }
        }

        results
    }

    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error> {
        if seq.as_bytes().is_empty() {
            tracing::debug!(target: trace_categories::INPUT, "ignoring empty binding trigger");
            return Ok(());
        }
        let Some(event) = translate_action_to_reedline_event(&action) else {
            return Err(std::io::Error::other(KeyError::UnsupportedKeyAction(
                action,
            )));
        };
        let seq = canonical(&seq);

        if let Some((modifiers, key_code)) = lift_key(seq.as_bytes()) {
            // A key holds either an action or a macro, never both, and reedline dispatches
            // single keys from its own map.
            self.remove_byte_binding(seq.as_bytes());
            self.update(|bindings| bindings.add_binding(modifiers, key_code, event.clone()));
        } else {
            self.insert_byte_binding(seq.into_bytes(), Binding::Action(action));
        }

        Ok(())
    }

    fn try_unbind(&mut self, seq: KeySequence) -> bool {
        let seq = canonical(&seq);
        let removed = self.remove_byte_binding(seq.as_bytes());
        self.remove_base_binding(&seq) || removed
    }

    fn define_macro(&mut self, seq: KeySequence, target: KeyMacro) -> Result<(), std::io::Error> {
        if seq.as_bytes().is_empty() {
            tracing::debug!(target: trace_categories::INPUT, "ignoring empty macro trigger");
            return Ok(());
        }
        let seq = canonical(&seq);

        // A key holds either an action or a macro, never both; on a single key the action
        // it replaces lives in reedline's own map.
        self.remove_base_binding(&seq);
        self.insert_byte_binding(seq.into_bytes(), Binding::Macro(target));

        Ok(())
    }

    fn get_macros(&self) -> BTreeMap<KeySequence, KeyMacro> {
        use radix_trie::TrieCommon;

        self.bindings_by_bytes
            .iter()
            .filter_map(|(bytes, binding)| match binding {
                Binding::Macro(target) => Some((KeySequence::from(bytes.clone()), target.clone())),
                Binding::Action(_) => None,
            })
            .collect()
    }
}

/// What a key or byte sequence encountered while resolving a macro turned out to be.
enum Lookup {
    /// Another macro, whose bytes get spliced into the replay. Owned, so the bindings can be
    /// borrowed again once the matched input has been consumed.
    Macro(KeySequence, KeyMacro),
    /// A bound editor event.
    Event(reedline::ReedlineEvent),
    /// An unbound character, inserted as text.
    Text(char),
    /// An unbound key or an undecodable byte in a macro body. Its bytes are consumed all
    /// the same, so resolution always advances.
    Nothing,
}

/// Accumulates the events a macro body resolves to.
#[derive(Default)]
struct Resolution {
    events: Vec<reedline::ReedlineEvent>,
    /// Literal text not yet flushed into an insert event.
    literal: String,
}

impl Resolution {
    fn flush_literal(&mut self) {
        if !self.literal.is_empty() {
            self.events.push(reedline::ReedlineEvent::Edit(vec![
                reedline::EditCommand::InsertString(std::mem::take(&mut self.literal)),
            ]));
        }
    }

    fn push(&mut self, event: reedline::ReedlineEvent) {
        if event != reedline::ReedlineEvent::None {
            self.flush_literal();
            self.events.push(event);
        }
    }
}

impl UpdatableBindings {
    fn insert_byte_binding(&mut self, bytes: Vec<u8>, binding: Binding) {
        self.longest_sequence = self.longest_sequence.max(bytes.len());
        self.bindings_by_bytes.insert(bytes, binding);
    }

    /// Removes the byte binding for `bytes`, if any; returns whether one was found.
    fn remove_byte_binding(&mut self, bytes: &[u8]) -> bool {
        use radix_trie::TrieCommon;

        let removed = self.bindings_by_bytes.remove(bytes).is_some();
        if removed && bytes.len() == self.longest_sequence {
            self.longest_sequence = self
                .bindings_by_bytes
                .keys()
                .map(Vec::len)
                .max()
                .unwrap_or(0);
        }

        removed
    }

    /// Whether some bound sequence is longer than `bytes` and starts with it. Walks only the
    /// bindings under that prefix.
    fn starts_a_longer_sequence(&self, bytes: &[u8]) -> bool {
        use radix_trie::TrieCommon;

        self.bindings_by_bytes
            .get_raw_descendant(bytes)
            .is_some_and(|subtrie| subtrie.keys().any(|key| key.len() > bytes.len()))
    }

    /// Whether the held input is a pressed escape key that the next key could still turn
    /// into a meta key: the terminal reports Esc and the key after it as two events, and
    /// `\e` then `f` is Alt+f. Only a pressed escape waits; one replayed from a macro body
    /// already has whatever follows it in the same stream.
    fn starts_a_meta_key(&self, bytes: &[u8]) -> bool {
        bytes == [0x1b]
            && self
                .pending
                .front()
                .is_some_and(|first| first.event.is_some())
    }

    /// Appends a terminal event to the same stream into which macro bodies are spliced. A
    /// key with no byte spelling, and every non-key event other than resize (bracketed
    /// paste today), is a matching barrier; resize goes straight to the edit mode.
    fn dispatch(&mut self, event: crossterm::event::Event) -> reedline::ReedlineEvent {
        if matches!(event, crossterm::event::Event::Resize(..)) {
            return self.edit_mode_event(event);
        }
        let bytes = match &event {
            crossterm::event::Event::Key(key) => input_sequence_for(key.modifiers, key.code)
                .map_or_else(VecDeque::new, |seq| seq.into_bytes().into()),
            _ => VecDeque::new(),
        };
        self.pending.push_back(PendingInput::native(event, bytes));
        // Each keystroke is a fresh user action, so it gets the whole budget; what a macro
        // leaves behind for a bound command carries the remainder in its replay instead.
        self.budget = self.max_replay;

        self.resolve_pending()
    }

    /// Fires a held lone escape's default meaning at once, leaving the key held for a meta
    /// key to complete; the miss that may follow does not repeat it. There is no
    /// key-sequence timeout, so waiting would leave a completion menu open until the next
    /// key. An escape rebound to anything else waits like any other prefix.
    fn fire_lone_escape_early(&mut self) -> reedline::ReedlineEvent {
        let Some(held) = self.pending.front() else {
            return reedline::ReedlineEvent::None;
        };
        if self.pending.len() != 1 || held.bytes != [0x1b] || held.fired {
            return reedline::ReedlineEvent::None;
        }
        let Some(event) = held.event.clone() else {
            return reedline::ReedlineEvent::None;
        };
        // Only the plain escape key resets editor state; anything else bound to it, a macro
        // included, waits like any other prefix.
        let Lookup::Event(fired) = self.translate(event) else {
            return reedline::ReedlineEvent::None;
        };
        if fired != reedline::ReedlineEvent::Esc {
            return reedline::ReedlineEvent::None;
        }

        if let Some(held) = self.pending.front_mut() {
            held.fired = true;
        }
        fired
    }

    /// A native terminal key's ordinary meaning: looked up by its byte spelling through the
    /// same [`Self::lookup_key`] a macro body goes through (so a binding on `A` is found
    /// though the terminal reports Shift+A, and the meta upper-case fallback applies
    /// either way), else whatever reedline's Emacs mode makes of it.
    fn translate(&mut self, event: crossterm::event::Event) -> Lookup {
        if let crossterm::event::Event::Key(key) = &event
            && let Some(seq) = input_sequence_for(key.modifiers, key.code)
            && let Some((modifiers, key_code)) = lift_key(seq.as_bytes())
            && let Some(lookup) = self.lookup_key(modifiers, key_code)
        {
            return lookup;
        }

        Lookup::Event(self.edit_mode_event(event))
    }

    /// What reedline's own Emacs edit mode makes of an event we have no binding for.
    fn edit_mode_event(&mut self, event: crossterm::event::Event) -> reedline::ReedlineEvent {
        match reedline::ReedlineRawEvent::try_from(event) {
            Ok(event) => self.edit_mode.parse_event(event),
            Err(()) => reedline::ReedlineEvent::None,
        }
    }

    /// Removes any binding reedline dispatches itself for `seq` (already canonical);
    /// returns whether one was found. Sequences that aren't a single key are held in
    /// [`Self::bindings_by_bytes`] instead, and removed from there directly.
    fn remove_base_binding(&mut self, seq: &KeySequence) -> bool {
        let Some((modifiers, key_code)) = lift_key(seq.as_bytes()) else {
            return false;
        };
        let found = self.bindings.find_binding(modifiers, key_code).is_some();
        if found {
            self.update(|bindings| {
                let _ = bindings.remove_binding(modifiers, key_code);
            });
        }

        found
    }

    /// The most bytes any matching step can need.
    fn matching_window(&self) -> usize {
        self.longest_sequence.max(MAX_KEY_LEN)
    }

    /// Resolves the shared stream until it needs more input or returns to the host. A macro
    /// body is pushed in front of the unconsumed stream, not resolved in a separate call, so
    /// that a trigger, escape sequence or character can span nested macro and keyboard
    /// boundaries.
    fn resolve_pending(&mut self) -> reedline::ReedlineEvent {
        let lookahead = self.matching_window();
        let mut resolution = Resolution::default();
        // Whether the last thing resolved was an escape on its own: a run of pressed
        // escapes pairs off rather than each prefixing the next key. Local to this pass,
        // which is the whole life of a run; the escape a miss releases is resolved in the
        // same pass as the one that missed it.
        let mut released_lone_escape = false;
        while !self.pending.is_empty() {
            self.pending.normalize(lookahead);
            let (bytes, at_barrier) = self.pending.matchable(lookahead);
            if !at_barrier
                && (self.starts_a_longer_sequence(&bytes)
                    || (!released_lone_escape && self.starts_a_meta_key(&bytes)))
            {
                // Something could still extend what is held, so wait for it.
                let escape = self.fire_lone_escape_early();
                resolution.push(escape);
                break;
            }

            let from_macro = self
                .pending
                .front()
                .is_some_and(|first| first.event.is_none());
            let Some((lookup, consumed)) = self.resolve_front(&bytes) else {
                continue;
            };
            // Before acting on it: `stop_at` decides by what is left pending, and a macro
            // body is spliced in front of that remainder.
            self.pending.consume(consumed);
            released_lone_escape = consumed == 1 && bytes.first() == Some(&0x1b);

            match lookup {
                Lookup::Event(event) => {
                    if self.stop_at(event, from_macro, &mut resolution) {
                        return combine(resolution.events);
                    }
                }
                Lookup::Text(character) => resolution.literal.push(character),
                Lookup::Macro(seq, target) => {
                    if !self.expand_macro(&seq, &target) {
                        return reedline::ReedlineEvent::None;
                    }
                }
                Lookup::Nothing => (),
            }
        }
        resolution.flush_literal();
        combine(resolution.events)
    }

    /// What the front of the stream resolves to, and how many bytes it takes: the longest
    /// bound sequence, else a native key's ordinary meaning, else the bytes read as a key.
    ///
    /// Always consumes at least one byte, or one native event with no byte spelling, so
    /// resolution advances. `None` means the front chunk carried nothing to match and was
    /// dropped, so the caller should look again.
    fn resolve_front(&mut self, bytes: &[u8]) -> Option<(Lookup, usize)> {
        if let Some(found) = self.lookup_raw_prefix(bytes) {
            return Some(found);
        }

        let first = self.pending.front()?;
        let (native, fired, native_len) = (first.event.clone(), first.fired, first.bytes.len());

        // A pressed Esc and the key after it are one meta key when the pair is bound. Only
        // a key reaching past the front event is read this way; one that ends there keeps
        // its own event below, with its exact modifiers.
        if native.is_some()
            && self
                .pending
                .leading_key(bytes)
                .is_some_and(|(_, len)| len > native_len)
        {
            let combined = self.lookup_bytes(bytes);
            // An unbound pair falls through: the escape takes its own meaning and the key
            // after it is dispatched afresh, as after any missed prefix. readline would
            // ring the bell and drop both.
            if !matches!(combined.0, Lookup::Nothing) {
                return Some(combined);
            }
        }

        match native {
            // A key already released while held must not fire a second time on the miss.
            Some(_) if fired => Some((Lookup::Event(reedline::ReedlineEvent::None), native_len)),
            Some(event) => Some((self.translate(event), native_len)),
            None if bytes.is_empty() => {
                // A chunk with neither bytes nor a native event; nothing produces one
                // today. Drop it so the loop advances.
                tracing::debug!(
                    target: trace_categories::INPUT,
                    "dropping a pending chunk with neither bytes nor an event"
                );
                self.pending.pop_front();
                None
            }
            None => Some(self.lookup_bytes(bytes)),
        }
    }

    /// Splices a macro body in front of the unconsumed stream, charging its length to this
    /// keystroke's remaining replay budget. Returns whether there was budget left for it.
    fn expand_macro(&mut self, seq: &KeySequence, target: &KeyMacro) -> bool {
        let Some(remaining) = self.budget.checked_sub(target.as_bytes().len()) else {
            // Abandon the whole keystroke, resolved events included, rather than leave the
            // buffer holding however much of a runaway macro fit in the budget.
            tracing::warn!(
                target: trace_categories::INPUT,
                "macro '{seq}' exhausted this keystroke's replay budget; abandoning it"
            );
            self.pending.clear();
            return false;
        };
        self.budget = remaining;
        if !target.as_bytes().is_empty() {
            self.pending
                .push_front(PendingInput::macro_bytes(target.as_bytes()));
        }

        true
    }

    /// Records `event` into `resolution`, stopping there if reedline would return to the
    /// host on it and forget what is still pending. A stop can be nested inside a
    /// composite event (`\M-#` ends in an accept-line), so both the decision and the
    /// hand-over work on the flattened view; an event that does not stop is kept as it
    /// is. Returns whether resolution stopped.
    fn stop_at(
        &mut self,
        event: reedline::ReedlineEvent,
        from_macro: bool,
        resolution: &mut Resolution,
    ) -> bool {
        let flat = flatten(event.clone());
        let host = flat
            .iter()
            .any(|event| matches!(event, reedline::ReedlineEvent::ExecuteHostCommand(_)));
        let enter = flat
            .iter()
            .any(|event| matches!(event, reedline::ReedlineEvent::Enter));
        // A bound command reached from a macro always stops: reedline suspends its read
        // loop for it. Either kind of stop also matters with input still pending, because
        // reedline discards it.
        if !((host && from_macro) || (!self.pending.is_empty() && (host || enter))) {
            resolution.push(event);
            return false;
        }

        let mut flat = flat.into_iter();
        while let Some(event) = flat.next() {
            let action = match event {
                reedline::ReedlineEvent::ExecuteHostCommand(command) => {
                    DeferredAction::RunCommand(HostCommand::decode(&command))
                }
                reedline::ReedlineEvent::Enter => DeferredAction::AcceptLine,
                other => {
                    resolution.push(other);
                    continue;
                }
            };
            let replay = (!self.pending.is_empty()).then(|| DeferredReplay {
                pending: std::mem::take(&mut self.pending),
                budget: self.budget,
            });
            let stop = match (action, replay) {
                // A bound command with nothing behind it is just that command.
                (DeferredAction::RunCommand(command), None) => {
                    reedline::ReedlineEvent::ExecuteHostCommand(command.encode())
                }
                (action, replay) => {
                    let id = self.deferred.len();
                    self.deferred.push(Some(Deferred { action, replay }));
                    reedline::ReedlineEvent::ExecuteHostCommand(HostCommand::Deferred(id).encode())
                }
            };
            resolution.push(stop);
            // reedline returns to the host on the stop, so events nested after it in the
            // same bound event never run. Nothing binds such an event today.
            let dropped: Vec<_> = flat.collect();
            if !dropped.is_empty() {
                tracing::debug!(
                    target: trace_categories::INPUT,
                    "dropping events nested after a stop: {dropped:?}"
                );
            }
            return true;
        }

        false
    }

    /// Interprets the start of `bytes` as a key, as readline does when replaying a macro,
    /// returning what it maps to and how many bytes it consumed (always at least one).
    /// `bytes` must be nonempty, and [`Self::lookup_raw_prefix`] must already have found
    /// no bound sequence here.
    fn lookup_bytes(&self, bytes: &[u8]) -> (Lookup, usize) {
        let Some(&first) = bytes.first() else {
            debug_assert!(false, "lookup_bytes needs bytes to match");
            return (Lookup::Nothing, 1);
        };

        if let Some(((modifiers, key_code), consumed)) = self.pending.leading_key(bytes) {
            let lookup = self
                .lookup_key(modifiers, key_code)
                .unwrap_or_else(|| match key_code {
                    reedline::KeyCode::Char(c) if modifiers.is_empty() => Lookup::Text(c),
                    reedline::KeyCode::Esc if modifiers.is_empty() => {
                        Lookup::Event(reedline::ReedlineEvent::Esc)
                    }
                    _ => {
                        tracing::debug!(
                            target: trace_categories::INPUT,
                            "key is bound to nothing: {modifiers:?} {key_code:?}"
                        );
                        Lookup::Nothing
                    }
                });
            return (lookup, consumed);
        }

        tracing::debug!(
            target: trace_categories::INPUT,
            "skipping byte that starts no key in macro: 0x{first:02x}"
        );
        (Lookup::Nothing, 1)
    }

    /// Finds the longest bound byte sequence at the start of `bytes`, with its length.
    /// A shorter one is tried when the longest would split a native terminal event.
    fn lookup_raw_prefix(&self, bytes: &[u8]) -> Option<(Lookup, usize)> {
        use radix_trie::TrieCommon;

        let mut prefix = bytes;
        loop {
            let subtrie = self.bindings_by_bytes.get_ancestor(prefix)?;
            let key = subtrie.key().filter(|key| !key.is_empty())?;
            if self.pending.is_boundary(key.len()) {
                // `get_ancestor` only returns nodes that have a value.
                let binding = subtrie.value()?;
                return Some((lookup_for(key, binding), key.len()));
            }
            prefix = &key[..key.len() - 1];
        }
    }

    /// Looks up a single key: first as a macro trigger, then in the base bindings. A meta
    /// key with an upper-case character falls back to its lower-case binding. `None` means
    /// the key is unbound.
    fn lookup_key(
        &self,
        modifiers: reedline::KeyModifiers,
        key_code: reedline::KeyCode,
    ) -> Option<Lookup> {
        // Finds a macro reached through the upper-case fallback below, whose spelling is
        // not the one in the input stream (that one is found by `lookup_raw_prefix`).
        if let Some(seq) = key_sequence_for(modifiers, key_code)
            && let Some(Binding::Macro(target)) = self.bindings_by_bytes.get(seq.as_bytes())
        {
            return Some(Lookup::Macro(seq, target.clone()));
        }

        if let Some(event) = self.bindings.find_binding(modifiers, key_code) {
            return Some(Lookup::Event(event));
        }

        match key_code {
            reedline::KeyCode::Char(c)
                if modifiers.contains(reedline::KeyModifiers::ALT) && c.is_ascii_uppercase() =>
            {
                self.lookup_key(modifiers, reedline::KeyCode::Char(c.to_ascii_lowercase()))
            }
            _ => None,
        }
    }
}

/// What a bound sequence resolves to when it is met in the input stream.
fn lookup_for(key: &[u8], binding: &Binding) -> Lookup {
    match binding {
        // `bind` refuses an action with no event, so the fallback is never taken; it is
        // here so a match always consumes.
        Binding::Action(action) => {
            translate_action_to_reedline_event(action).map_or(Lookup::Nothing, Lookup::Event)
        }
        Binding::Macro(target) => Lookup::Macro(KeySequence::from(key.to_vec()), target.clone()),
    }
}

#[cfg(test)]
pub(super) fn press(
    bindings: &mut UpdatableBindings,
    modifiers: reedline::KeyModifiers,
    key_code: reedline::KeyCode,
) -> Result<reedline::ReedlineEvent, std::io::Error> {
    send(
        bindings,
        crossterm::event::Event::Key(crossterm::event::KeyEvent::new(key_code, modifiers)),
    )
}

#[cfg(test)]
pub(super) fn send(
    bindings: &mut UpdatableBindings,
    event: crossterm::event::Event,
) -> Result<reedline::ReedlineEvent, std::io::Error> {
    let raw = reedline::ReedlineRawEvent::try_from(event)
        .map_err(|()| std::io::Error::other("not a valid raw event"))?;

    Ok(reedline::EditMode::parse_event(bindings, raw))
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::interfaces::InputFunction;
    use interfaces::KeyBindings as _;
    use reedline::{EditCommand, KeyCode, KeyModifiers, ReedlineEvent};

    fn control_key(character: char) -> KeySequence {
        KeySequence::from(vec![brush_parser::readline_binding::control_byte(
            character as u8,
        )])
    }

    fn alt_key(character: char) -> KeySequence {
        let mut bytes = vec![0x1b];
        bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
        KeySequence::from(bytes)
    }

    /// The replay budget, in bytes, the tests below run with. Termination is a property of
    /// the budget existing, not of its size, so a small one proves the same thing in a
    /// fraction of the time; [`the_shipped_budget_is_the_default`] pins the real value.
    const TEST_MAX_REPLAY: usize = 60;

    fn new_bindings() -> UpdatableBindings {
        let mut bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
        bindings.set_max_replay(TEST_MAX_REPLAY);
        bindings
    }

    #[test]
    fn the_shipped_budget_is_the_default() {
        // `new_bindings` lowers the budget for speed; a running shell must still get the
        // shipped ceiling, and nothing else here would notice if that changed.
        let bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
        assert_eq!(bindings.max_replay, MAX_MACRO_REPLAY_BYTES);
        assert_eq!(bindings.budget, MAX_MACRO_REPLAY_BYTES);
    }

    fn define(
        bindings: &mut UpdatableBindings,
        seq: KeySequence,
        body: &[u8],
    ) -> Result<(), std::io::Error> {
        bindings.define_macro(seq, KeyMacro::from(body.to_vec()))
    }

    fn press_ctrl(
        bindings: &mut UpdatableBindings,
        character: char,
    ) -> Result<ReedlineEvent, std::io::Error> {
        press(bindings, KeyModifiers::CONTROL, KeyCode::Char(character))
    }

    fn base_binding(
        bindings: &UpdatableBindings,
        key_code: KeyCode,
    ) -> Result<ReedlineEvent, std::io::Error> {
        bindings
            .bindings
            .find_binding(KeyModifiers::NONE, key_code)
            .ok_or_else(|| std::io::Error::other(std::format!("{key_code:?} is not bound")))
    }

    /// Claims the deferred record named by the bound command a resolved event ends in.
    fn deferred(
        bindings: &mut UpdatableBindings,
        event: &ReedlineEvent,
    ) -> Result<Deferred, std::io::Error> {
        let encoded = match event {
            ReedlineEvent::ExecuteHostCommand(encoded) => encoded,
            ReedlineEvent::Multiple(events) => match events.last() {
                Some(ReedlineEvent::ExecuteHostCommand(encoded)) => encoded,
                _ => return Err(std::io::Error::other("event does not end in a command")),
            },
            _ => return Err(std::io::Error::other("event is not a command")),
        };
        match HostCommand::decode(encoded) {
            // Straight out of the store rather than through `end_read`, which would also
            // end the read the test is still in the middle of.
            HostCommand::Deferred(id) => bindings
                .deferred
                .get_mut(id)
                .and_then(Option::take)
                .ok_or_else(|| std::io::Error::other(std::format!("no deferred record {id}"))),
            other @ HostCommand::Command(_) => Err(std::io::Error::other(std::format!(
                "bound command is not a deferred record: {other:?}"
            ))),
        }
    }

    /// A replay of `bytes` with the budget a keystroke that replayed `spent` macro bytes
    /// before stopping would have left.
    fn replay(bytes: &[u8], spent: usize) -> DeferredReplay {
        DeferredReplay::new(bytes, TEST_MAX_REPLAY - spent)
    }

    fn host_event(command: &str) -> ReedlineEvent {
        ReedlineEvent::ExecuteHostCommand(HostCommand::Command(command.to_owned()).encode())
    }

    fn insert(text: &str) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![EditCommand::InsertString(text.to_owned())])
    }

    fn edit(command: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![command])
    }

    #[test]
    fn native_modified_keys_do_not_alias_unmodified_bindings() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        for (modifiers, code) in [
            (KeyModifiers::SHIFT, KeyCode::Enter),
            (KeyModifiers::CONTROL, KeyCode::Backspace),
            (
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                KeyCode::Char('a'),
            ),
        ] {
            let expected = bindings
                .bindings
                .find_binding(modifiers, code)
                .ok_or_else(|| std::io::Error::other("missing native modified binding"))?;
            assert_eq!(press(&mut bindings, modifiers, code)?, expected);

            let alias = key_sequence_for(modifiers, code)
                .ok_or_else(|| std::io::Error::other("missing canonical alias"))?;
            define(&mut bindings, alias.clone(), b"wrong")?;
            assert_eq!(press(&mut bindings, modifiers, code)?, expected);
            bindings.try_unbind(alias);
        }
        Ok(())
    }

    #[test]
    fn paste_flushes_a_pending_prefix_in_input_order() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, KeySequence::from(b"xy".to_vec()), b"MATCH")?;
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('x'))?,
            ReedlineEvent::None
        );
        assert_eq!(
            send(
                &mut bindings,
                crossterm::event::Event::Paste("PASTED".to_owned())
            )?,
            ReedlineEvent::Multiple(vec![edit(EditCommand::InsertChar('x')), insert("PASTED")])
        );
        assert!(bindings.pending.is_empty());
        Ok(())
    }

    #[test]
    fn paste_behind_a_host_prefix_is_deferred_without_overtaking_it() -> Result<(), std::io::Error>
    {
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("short".to_owned()),
        )?;
        define(&mut bindings, KeySequence::from(b"\x14x".to_vec()), b"long")?;
        assert_eq!(press_ctrl(&mut bindings, 't')?, ReedlineEvent::None);
        let event = send(
            &mut bindings,
            crossterm::event::Event::Paste("PASTED".to_owned()),
        )?;
        let stopped = deferred(&mut bindings, &event)?;
        assert_eq!(
            stopped.action,
            DeferredAction::RunCommand(HostCommand::Command("short".to_owned()))
        );
        let replay = stopped
            .replay
            .ok_or_else(|| std::io::Error::other("paste was lost"))?;
        bindings.end_read(None);
        assert_eq!(bindings.resolve_replay(replay), insert("PASTED"));
        Ok(())
    }

    #[test]
    fn repeated_child_macros_have_linear_normalization_work() -> Result<(), std::io::Error> {
        // The whole-resolution consequence of the bound above: one child macro spliced per
        // outer byte still resolves, and in time proportional to the body's length.
        for repetitions in [128, 512, 1024, 2048] {
            // The shipped budget, not the lowered one the other tests use: a body of a few
            // thousand bytes, wide rather than recursive, must fit in it, which is the
            // claim this makes, and 2048 expansions of it cost a fraction of a second.
            let mut bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
            define(&mut bindings, control_key('t'), b"a")?;
            define(&mut bindings, control_key('g'), &vec![0x14; repetitions])?;
            assert_eq!(
                press_ctrl(&mut bindings, 'g')?,
                insert(&"a".repeat(repetitions))
            );
        }
        Ok(())
    }

    #[test]
    fn empty_triggers_are_ignored_without_suppressing_empty_bodies() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        let empty = KeySequence::from(Vec::new());
        define(&mut bindings, empty.clone(), b"")?;
        bindings.bind(
            empty.clone(),
            KeyAction::DoInputFunction(InputFunction::AcceptLine),
        )?;
        assert!(!bindings.get_macros().contains_key(&empty));
        assert!(!bindings.get_current().contains_key(&empty));

        define(&mut bindings, control_key('t'), b"")?;
        define(&mut bindings, control_key('g'), b"\x14text\x14")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("text"));
        assert_eq!(press_ctrl(&mut bindings, 't')?, ReedlineEvent::None);
        Ok(())
    }

    #[test]
    fn macro_trigger_spans_nested_and_keyboard_input() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, KeySequence::from(b"xy".to_vec()), b"OK")?;
        define(&mut bindings, control_key('t'), b"x")?;
        define(&mut bindings, control_key('g'), b"\x14y")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("OK"));

        assert_eq!(press_ctrl(&mut bindings, 't')?, ReedlineEvent::None);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('y'))?,
            insert("OK")
        );
        Ok(())
    }

    #[test]
    fn decoded_keys_and_utf8_span_nested_macro_chunks() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x1b[A".to_vec()),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;
        define(&mut bindings, control_key('t'), b"\x18\x1bO")?;
        define(&mut bindings, control_key('g'), b"\x14A")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, ReedlineEvent::ClearScreen);

        define(&mut bindings, control_key('t'), b"\xc3")?;
        define(&mut bindings, control_key('g'), b"\x14\xa9")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("é"));
        Ok(())
    }

    #[test]
    fn prefix_fallback_defers_native_keys_and_resolves_after_rebinding()
    -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("short".to_owned()),
        )?;
        bindings.bind(
            KeySequence::from(b"\x14\x12".to_vec()),
            KeyAction::ShellCommand("long".to_owned()),
        )?;
        assert_eq!(press_ctrl(&mut bindings, 't')?, ReedlineEvent::None);
        let event = press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('z'))?;
        let stopped = deferred(&mut bindings, &event)?;
        assert_eq!(
            stopped.action,
            DeferredAction::RunCommand(HostCommand::Command("short".to_owned()))
        );
        let replay = stopped
            .replay
            .ok_or_else(|| std::io::Error::other("native key was not deferred"))?;
        bindings.end_read(None);
        define(&mut bindings, KeySequence::from(b"z".to_vec()), b"rebound")?;
        assert_eq!(bindings.resolve_replay(replay), insert("rebound"));
        Ok(())
    }

    #[test]
    fn prefix_fallback_preserves_lossy_native_key_across_accept_line() -> Result<(), std::io::Error>
    {
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::DoInputFunction(InputFunction::AcceptLine),
        )?;
        define(
            &mut bindings,
            KeySequence::from(b"\x14\x12".to_vec()),
            b"long",
        )?;
        assert_eq!(press_ctrl(&mut bindings, 't')?, ReedlineEvent::None);
        let event = press(&mut bindings, KeyModifiers::SHIFT, KeyCode::Enter)?;
        let stopped = deferred(&mut bindings, &event)?;
        assert_eq!(stopped.action, DeferredAction::AcceptLine);
        let replay = stopped
            .replay
            .ok_or_else(|| std::io::Error::other("modified native key was not deferred"))?;
        bindings.end_read(None);
        assert_eq!(
            bindings.resolve_replay(replay),
            edit(EditCommand::InsertNewline)
        );
        Ok(())
    }

    #[test]
    fn a_macro_that_loops_through_a_bound_command_runs_out() -> Result<(), std::io::Error> {
        // "\C-t": "\C-x a \C-t" with \C-x bound to a command: every round trip through the
        // command expands the macro once more, so the budget has to survive the deferral or
        // the loop never ends. bash recurses here without limit.
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('x'),
            KeyAction::ShellCommand("command".to_owned()),
        )?;
        let body = b"\x18a\x14";
        define(&mut bindings, control_key('t'), body)?;

        let mut event = press_ctrl(&mut bindings, 't')?;
        let mut rounds = 0;
        while let Ok(decoded) = deferred(&mut bindings, &event) {
            let replay = decoded
                .replay
                .ok_or_else(|| std::io::Error::other("command with nothing left to replay"))?;
            rounds += 1;
            assert!(rounds <= TEST_MAX_REPLAY, "replay chain does not terminate");
            bindings.end_read(None);
            event = bindings.resolve_replay(replay);
        }

        // The round that runs out is abandoned whole, text and all. Each round replays the
        // body once, so the budget buys that many rounds.
        assert_eq!(event, ReedlineEvent::None);
        assert_eq!(rounds, TEST_MAX_REPLAY / body.len());
        Ok(())
    }

    #[test]
    fn unreturned_replay_records_are_dropped_at_the_read_boundary() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("command".to_owned()),
        )?;
        define(&mut bindings, control_key('g'), b"\x14discarded")?;
        let _ = press_ctrl(&mut bindings, 'g')?;
        assert_eq!(bindings.deferred.len(), 1);
        assert!(bindings.end_read(None).is_none());
        assert!(bindings.end_read(Some(0)).is_none());
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('z'))?,
            edit(EditCommand::InsertChar('z'))
        );
        Ok(())
    }

    #[test]
    fn macro_resolves_literal_text() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"literal text")?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("literal text"));
        Ok(())
    }

    #[test]
    fn macro_resolves_multibyte_literal_text() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), "héllo wörld".as_bytes())?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("héllo wörld"));
        Ok(())
    }

    #[test]
    fn macro_resolves_mixed_text_and_commands() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(
            &mut bindings,
            control_key('g'),
            b"discarded\x01\x0breplacement\x0d",
        )?;

        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            ReedlineEvent::Multiple(vec![
                insert("discarded"),
                edit(EditCommand::MoveToLineStart { select: false }),
                edit(EditCommand::KillLine),
                insert("replacement"),
                ReedlineEvent::Enter,
            ])
        );
        Ok(())
    }

    #[test]
    fn macro_resolves_meta_binding() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"\x1bb")?;

        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            edit(EditCommand::MoveWordLeft { select: false })
        );
        Ok(())
    }

    #[test]
    fn macro_meta_binding_prefers_exact_case() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            alt_key('B'),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;
        define(&mut bindings, control_key('g'), b"\x1bB")?;
        define(&mut bindings, control_key('t'), b"\x1bb")?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, ReedlineEvent::ClearScreen);
        assert_eq!(
            press_ctrl(&mut bindings, 't')?,
            edit(EditCommand::MoveWordLeft { select: false })
        );

        Ok(())
    }

    #[test]
    fn macro_meta_binding_falls_back_to_lowercase() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"\x1bB")?;

        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            edit(EditCommand::MoveWordLeft { select: false })
        );
        Ok(())
    }

    #[test]
    fn macro_resolves_bare_escape() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"\x1b")?;

        let expected = base_binding(&bindings, KeyCode::Esc).unwrap_or(ReedlineEvent::Esc);
        assert_eq!(press_ctrl(&mut bindings, 'g')?, expected);
        Ok(())
    }

    #[test]
    fn macro_resolves_terminal_escape_sequences_to_keys() -> Result<(), std::io::Error> {
        // Both the CSI and SS3 spellings must work whatever terminfo says, as they do in
        // readline's default keymap.
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"\x1b[A")?;
        define(&mut bindings, control_key('t'), b"\x1bOD\x1b[3~")?;

        let up = base_binding(&bindings, KeyCode::Up)?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, up);

        let left = base_binding(&bindings, KeyCode::Left)?;
        let delete = base_binding(&bindings, KeyCode::Delete)?;
        assert_eq!(
            press_ctrl(&mut bindings, 't')?,
            ReedlineEvent::Multiple(vec![left, delete])
        );
        Ok(())
    }

    #[test]
    fn macro_drops_unbound_control_bytes() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        // Neither \C-\ nor \C-^ is bound in the default emacs keymap.
        define(&mut bindings, control_key('t'), b"a\x1cb\x1ec")?;

        assert_eq!(press_ctrl(&mut bindings, 't')?, insert("abc"));
        Ok(())
    }

    #[test]
    fn macro_skips_undecodable_bytes() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('t'), b"a\xffb")?;

        assert_eq!(press_ctrl(&mut bindings, 't')?, insert("ab"));
        Ok(())
    }

    #[test]
    fn empty_macro_resolves_to_nothing_and_shadows_base_binding() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('l'), b"")?;

        assert_eq!(press_ctrl(&mut bindings, 'l')?, ReedlineEvent::None);
        assert!(!bindings.get_current().contains_key(&control_key('l')));
        Ok(())
    }

    #[test]
    fn empty_byte_macro_consumes_bytes_during_replay() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, KeySequence::from(b"xy".to_vec()), b"")?;
        define(&mut bindings, control_key('g'), b"xyA")?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("A"));
        Ok(())
    }

    #[test]
    fn longest_raw_prefix_wins() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"xy".to_vec()),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;
        define(&mut bindings, KeySequence::from(b"xyz".to_vec()), b"long")?;
        define(&mut bindings, control_key('g'), b"xyz xy ")?;

        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            ReedlineEvent::Multiple(vec![
                insert("long "),
                ReedlineEvent::ClearScreen,
                insert(" ")
            ])
        );

        Ok(())
    }

    #[test]
    fn macro_resolves_existing_raw_binding() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        let raw_sequence = b"\x1b[0;1A".to_vec();
        bindings.bind(
            KeySequence::from(raw_sequence.clone()),
            KeyAction::ShellCommand("raw-command".to_owned()),
        )?;
        define(&mut bindings, control_key('g'), &raw_sequence)?;

        // Nothing follows the command in the body, so it is returned as itself rather than
        // wrapped in a record with nothing to carry.
        assert_eq!(press_ctrl(&mut bindings, 'g')?, host_event("raw-command"));

        Ok(())
    }

    #[test]
    fn bound_command_defers_remaining_bytes() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x1fA1\x07".to_vec()),
            KeyAction::ShellCommand("__atuin".to_owned()),
        )?;
        let body = b"\x18\x1fA1\x07\x18\x1fA0\x07";
        define(&mut bindings, control_key('r'), body)?;

        let event = press_ctrl(&mut bindings, 'r')?;
        assert!(matches!(event, ReedlineEvent::ExecuteHostCommand(_)));
        assert_eq!(
            deferred(&mut bindings, &event)?,
            Deferred {
                action: DeferredAction::RunCommand(HostCommand::Command("__atuin".to_owned())),
                replay: Some(replay(b"\x18\x1fA0\x07", body.len())),
            }
        );

        Ok(())
    }

    #[test]
    fn deferred_bytes_resolve_against_current_bindings() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        let trailer = KeySequence::from(b"\x18\x1fA0\x07".to_vec());
        bindings.bind(
            KeySequence::from(b"\x18\x1fA1\x07".to_vec()),
            KeyAction::ShellCommand("__atuin".to_owned()),
        )?;
        define(&mut bindings, trailer.clone(), b"")?;
        define(
            &mut bindings,
            control_key('r'),
            b"\x18\x1fA1\x07\x18\x1fA0\x07",
        )?;

        let event = press_ctrl(&mut bindings, 'r')?;
        let replay = deferred(&mut bindings, &event)?
            .replay
            .ok_or_else(|| std::io::Error::other("no bytes deferred past the bound command"))?;

        // The bound command rebinds the trailer (as atuin's accept path does) before the
        // deferred bytes are replayed.
        bindings.bind(
            trailer,
            KeyAction::DoInputFunction(InputFunction::AcceptLine),
        )?;
        assert_eq!(bindings.resolve_replay(replay), ReedlineEvent::Enter);

        Ok(())
    }

    #[test]
    fn bound_command_inside_nested_macro_defers_outer_remainder() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("inner-command".to_owned()),
        )?;
        let inner = b"\x14in";
        let outer = b"pre\x07out";
        define(&mut bindings, control_key('g'), inner)?;
        define(&mut bindings, control_key('r'), outer)?;

        let event = press_ctrl(&mut bindings, 'r')?;
        assert!(
            matches!(&event, ReedlineEvent::Multiple(events)
                if events.len() == 2 && events[0] == insert("pre")),
            "unexpected event: {event:?}"
        );
        assert_eq!(
            deferred(&mut bindings, &event)?,
            Deferred {
                action: DeferredAction::RunCommand(HostCommand::Command(
                    "inner-command".to_owned()
                )),
                // Two macros expanded before the stop: the outer and the nested one. The
                // inner remainder and the outer one are one chunk, having nothing to keep
                // them apart.
                replay: Some(replay(b"inout", inner.len() + outer.len())),
            }
        );

        Ok(())
    }

    #[test]
    fn chained_macros_are_spliced() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('t'), b"ab")?;
        define(&mut bindings, control_key('g'), b"\x14-\x14")?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("ab-ab"));
        Ok(())
    }

    #[test]
    fn macro_changes_are_seen_at_press_time() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('t'), b"one")?;
        define(&mut bindings, control_key('g'), b"\x14")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("one"));

        define(&mut bindings, control_key('t'), b"two")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("two"));
        Ok(())
    }

    #[test]
    fn self_referential_macro_resolves_the_same_way_every_time() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('l'), b"\x0c\x01")?;

        let first = press_ctrl(&mut bindings, 'l')?;
        define(&mut bindings, control_key('g'), b"zzz")?;
        define(&mut bindings, control_key('t'), b"zzz")?;
        let later = press_ctrl(&mut bindings, 'l')?;

        // Running out of budget abandons the whole keystroke rather than applying however
        // much of the body fit in it, and every press gets its own budget, so the same
        // press always resolves the same way.
        assert_eq!(first, ReedlineEvent::None);
        assert_eq!(first, later);
        Ok(())
    }

    #[test]
    fn a_wide_self_referential_macro_is_bounded_by_bytes_not_expansions()
    -> Result<(), std::io::Error> {
        // The shipped budget, with a body that is both wide and recursive: the budget is
        // spent in a handful of expansions, so this resolves in milliseconds. A budget
        // that counted expansions instead would replay thousands of copies of the body
        // first, and this test would take minutes.
        let mut bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
        let mut body = vec![b'a'; 4096];
        body.push(0x07);
        define(&mut bindings, control_key('g'), &body)?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, ReedlineEvent::None);
        Ok(())
    }

    #[test]
    fn mutually_recursive_macros_terminate() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"x\x14")?;
        define(&mut bindings, control_key('t'), b"y\x07")?;

        // The pair alternates until the budget is gone instead of looping forever, and
        // none of the text it produced on the way is applied.
        assert_eq!(press_ctrl(&mut bindings, 'g')?, ReedlineEvent::None);
        assert_eq!(press_ctrl(&mut bindings, 't')?, ReedlineEvent::None);
        Ok(())
    }

    #[test]
    fn non_macro_binding_replaces_macro_definition() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"literal text")?;
        bindings.bind(
            control_key('g'),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;

        assert!(bindings.get_macros().is_empty());
        assert_eq!(press_ctrl(&mut bindings, 'g')?, ReedlineEvent::ClearScreen);

        Ok(())
    }

    #[test]
    fn macro_definition_replaces_base_binding() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        assert_eq!(press_ctrl(&mut bindings, 'l')?, ReedlineEvent::ClearScreen);

        define(&mut bindings, control_key('l'), b"text")?;

        assert_eq!(press_ctrl(&mut bindings, 'l')?, insert("text"));
        assert!(!bindings.get_current().contains_key(&control_key('l')));
        assert_eq!(
            bindings.get_macros().get(&control_key('l')),
            Some(&KeyMacro::from(b"text".to_vec()))
        );
        Ok(())
    }

    #[test]
    fn macro_keys_are_not_listed_as_bindings() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x1fA1\x07".to_vec()),
            KeyAction::ShellCommand("__atuin".to_owned()),
        )?;
        define(
            &mut bindings,
            control_key('r'),
            b"\x18\x1fA1\x07\x18\x1fA0\x07",
        )?;
        let _ = press_ctrl(&mut bindings, 'r')?;

        let current = bindings.get_current();
        assert!(!current.contains_key(&control_key('r')));
        for action in current.values() {
            if let KeyAction::ShellCommand(cmd) = action {
                assert!(
                    !cmd.contains('\0'),
                    "listing leaked internal state: {cmd:?}"
                );
            }
        }

        Ok(())
    }

    #[test]
    fn unbind_removes_macro_and_restores_nothing() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('l'), b"text")?;

        assert!(bindings.try_unbind(control_key('l')));
        assert!(bindings.get_macros().is_empty());
        assert_eq!(press_ctrl(&mut bindings, 'l')?, ReedlineEvent::None);
        assert!(!bindings.try_unbind(control_key('l')));
        Ok(())
    }

    #[test]
    fn raw_byte_bindings_are_listed() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        let seq = KeySequence::from(b"\x18\x1fA1\x07".to_vec());
        bindings.bind(seq.clone(), KeyAction::ShellCommand("__atuin".to_owned()))?;

        let current = bindings.get_current();
        assert_eq!(seq.to_string(), r"\C-x\C-_A1\C-g");
        assert_eq!(
            current.get(&seq),
            Some(&KeyAction::ShellCommand("__atuin".to_owned()))
        );

        Ok(())
    }

    #[test]
    fn each_stop_carries_its_own_record() -> Result<(), std::io::Error> {
        // Two macros ending in the same command, pressed in one batch (reedline parses a
        // whole batch before acting on any of it): whichever reedline returns, its own tail
        // travels with it.
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("bound".to_owned()),
        )?;
        let body = b"\x14one";
        define(&mut bindings, control_key('r'), body)?;
        define(&mut bindings, control_key('g'), b"\x14two")?;

        let first = press_ctrl(&mut bindings, 'r')?;
        let second = press_ctrl(&mut bindings, 'g')?;
        assert_ne!(first, second);

        assert_eq!(
            deferred(&mut bindings, &first)?.replay,
            Some(replay(b"one", body.len()))
        );
        assert_eq!(
            deferred(&mut bindings, &second)?.replay,
            Some(replay(b"two", b"\x14two".len()))
        );

        Ok(())
    }

    #[test]
    fn deferred_replay_carries_the_remaining_budget() -> Result<(), std::io::Error> {
        // "\C-r": "\C-t\C-r" with \C-t bound to a command: replaying the trailing \C-r
        // must resume on what the keystroke had left, not on a fresh budget.
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("bound".to_owned()),
        )?;
        define(&mut bindings, control_key('r'), b"\x14\x12")?;

        let event = press_ctrl(&mut bindings, 'r')?;
        let replay = deferred(&mut bindings, &event)?
            .replay
            .ok_or_else(|| std::io::Error::other("no bytes deferred past the bound command"))?;
        assert_eq!(replay, self::replay(b"\x12", 2));

        bindings.end_read(None);
        let event = bindings.resolve_replay(replay);
        let replay = deferred(&mut bindings, &event)?
            .replay
            .ok_or_else(|| std::io::Error::other("no bytes deferred past the bound command"))?;
        assert_eq!(replay, self::replay(b"\x12", 4));

        Ok(())
    }

    #[test]
    fn macro_on_uppercase_letter_fires_from_keyboard() -> Result<(), std::io::Error> {
        // Terminals report a capital letter with the SHIFT modifier set; the letter itself
        // already carries the case, and `bind` never records shift.
        let mut bindings = new_bindings();
        define(&mut bindings, KeySequence::from(b"A".to_vec()), b"upper")?;
        define(&mut bindings, alt_key('F'), b"meta-upper")?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::SHIFT, KeyCode::Char('A'))?,
            insert("upper")
        );
        assert_eq!(
            press(
                &mut bindings,
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                KeyCode::Char('F')
            )?,
            insert("meta-upper")
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('a'))?,
            edit(EditCommand::InsertChar('a'))
        );

        Ok(())
    }

    #[test]
    fn macro_keys_are_canonicalized() -> Result<(), std::io::Error> {
        // The terminal reports both spellings of the up arrow as the same key, so they name
        // the same macro; other byte sequences are compared as-is.
        let mut bindings = new_bindings();
        define(&mut bindings, KeySequence::from(b"\x1bOA".to_vec()), b"one")?;
        define(&mut bindings, KeySequence::from(b"\x1b[A".to_vec()), b"two")?;
        assert_eq!(bindings.get_macros().len(), 1);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Up)?,
            insert("two")
        );

        assert!(bindings.try_unbind(KeySequence::from(b"\x1bOA".to_vec())));
        assert!(bindings.get_macros().is_empty());

        define(
            &mut bindings,
            KeySequence::from(b"\x18\x1fA0\x07".to_vec()),
            b"",
        )?;
        assert!(bindings.try_unbind(KeySequence::from(b"\x18\x1fA0\x07".to_vec())));
        assert!(!bindings.try_unbind(KeySequence::from(b"\x18\x1fA0".to_vec())));

        Ok(())
    }

    #[test]
    fn redefining_a_macro_identically_changes_nothing() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"text\x01")?;
        let first = press_ctrl(&mut bindings, 'g')?;

        define(&mut bindings, control_key('g'), b"text\x01")?;
        define(&mut bindings, control_key('t'), b"other")?;
        let again = press_ctrl(&mut bindings, 'g')?;

        assert_eq!(first, again);
        assert_eq!(bindings.get_macros().len(), 2);

        Ok(())
    }

    #[test]
    fn indirect_self_reference_through_deferred_replay_terminates() -> Result<(), std::io::Error> {
        // A: command, then B. B: text, then A. Each round trip replays both two-byte
        // bodies, so the mutual reference runs out instead of cycling forever.
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("bound".to_owned()),
        )?;
        define(&mut bindings, control_key('r'), b"\x14\x07")?;
        define(&mut bindings, control_key('g'), b"x\x12")?;

        let mut event = press_ctrl(&mut bindings, 'r')?;
        let mut rounds = 0;
        while let Ok(decoded) = deferred(&mut bindings, &event) {
            let replay = decoded
                .replay
                .ok_or_else(|| std::io::Error::other("command with nothing left to replay"))?;
            rounds += 1;
            assert!(rounds <= TEST_MAX_REPLAY, "replay chain does not terminate");
            bindings.end_read(None);
            event = bindings.resolve_replay(replay);
        }

        assert_eq!(event, ReedlineEvent::None);
        assert_eq!(rounds, TEST_MAX_REPLAY / 4);
        Ok(())
    }

    #[test]
    fn modified_cursor_keys_bind_and_replay() -> Result<(), std::io::Error> {
        // The `.inputrc` staples: ctrl-arrow and ctrl-delete, plus an alt-arrow.
        let mut bindings = new_bindings();
        let ctrl_right = KeySequence::from(b"\x1b[1;5C".to_vec());
        let ctrl_delete = KeySequence::from(b"\x1b[3;5~".to_vec());
        let alt_left = KeySequence::from(b"\x1b[1;3D".to_vec());
        bindings.bind(
            ctrl_right.clone(),
            KeyAction::DoInputFunction(InputFunction::ForwardWord),
        )?;
        bindings.bind(
            ctrl_delete.clone(),
            KeyAction::DoInputFunction(InputFunction::KillWord),
        )?;
        bindings.bind(
            alt_left,
            KeyAction::DoInputFunction(InputFunction::BackwardWord),
        )?;

        // Bound as keys, not raw bytes: they fire from the keyboard and list as bytes.
        assert_eq!(
            press(&mut bindings, KeyModifiers::CONTROL, KeyCode::Right)?,
            edit(EditCommand::MoveWordRight { select: false })
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::CONTROL, KeyCode::Delete)?,
            edit(EditCommand::CutWordRight)
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Left)?,
            edit(EditCommand::MoveWordLeft { select: false })
        );
        let current = bindings.get_current();
        assert_eq!(
            current.get(&ctrl_right),
            Some(&KeyAction::DoInputFunction(InputFunction::ForwardWord))
        );
        assert_eq!(
            current.get(&ctrl_delete),
            Some(&KeyAction::DoInputFunction(InputFunction::KillWord))
        );

        // And they replay from a macro body.
        define(&mut bindings, control_key('g'), b"\x1b[1;5C\x1b[1;3D")?;
        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            ReedlineEvent::Multiple(vec![
                edit(EditCommand::MoveWordRight { select: false }),
                edit(EditCommand::MoveWordLeft { select: false }),
            ])
        );

        Ok(())
    }

    #[test]
    fn raw_binding_fires_from_the_keyboard() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        // The first key is a prefix of the sequence: held, nothing happens yet.
        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(
            press_ctrl(&mut bindings, 'r')?,
            ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );

        Ok(())
    }

    #[test]
    fn multi_key_macro_fires_from_the_keyboard() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(
            &mut bindings,
            KeySequence::from(b"\x18\x12".to_vec()),
            b"text",
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(press_ctrl(&mut bindings, 'r')?, insert("text"));

        Ok(())
    }

    #[test]
    fn held_prefix_keys_are_released_on_a_miss() -> Result<(), std::io::Error> {
        // \C-a is bound by default; with \C-a\C-r bound too, \C-a has to wait. When the
        // next key doesn't complete the sequence, both keys take their ordinary meaning.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x01\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'a')?, ReedlineEvent::None);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('z'))?,
            ReedlineEvent::Multiple(vec![
                edit(EditCommand::MoveToLineStart { select: false }),
                edit(EditCommand::InsertChar('z')),
            ])
        );

        // Nothing is left held.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('q'))?,
            edit(EditCommand::InsertChar('q'))
        );

        Ok(())
    }

    #[test]
    fn keys_that_start_no_sequence_are_not_held() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(
            press_ctrl(&mut bindings, 'a')?,
            edit(EditCommand::MoveToLineStart { select: false })
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('a'))?,
            edit(EditCommand::InsertChar('a'))
        );

        Ok(())
    }

    #[test]
    fn a_complete_sequence_waits_for_a_longer_one() -> Result<(), std::io::Error> {
        // \C-x is both a macro key and the start of a longer raw binding. As in readline
        // (and as in macro resolution), the longest match wins: the key waits, the longer
        // sequence fires if it completes, and otherwise the macro fires ahead of the key
        // that missed.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;
        define(&mut bindings, control_key('x'), b"short")?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(
            press_ctrl(&mut bindings, 'r')?,
            ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('z'))?,
            ReedlineEvent::Multiple(vec![insert("short"), edit(EditCommand::InsertChar('z'))])
        );

        Ok(())
    }

    #[test]
    fn a_macro_key_fires_after_a_missed_prefix() -> Result<(), std::io::Error> {
        // \C-a is held for \C-a\C-e; \C-t doesn't complete it. \C-a takes its ordinary
        // meaning and \C-t is dispatched afresh, so its macro fires (checked against bash).
        // fzf plus atuin is the everyday case: Esc, held for fzf's \ec, then atuin's \C-r.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x01\x05".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;
        define(&mut bindings, control_key('t'), b"MACRO")?;
        define(&mut bindings, alt_key('c'), b"fzf")?;
        define(&mut bindings, control_key('r'), b"atuin")?;

        assert_eq!(press_ctrl(&mut bindings, 'a')?, ReedlineEvent::None);
        assert_eq!(
            press_ctrl(&mut bindings, 't')?,
            ReedlineEvent::Multiple(vec![
                edit(EditCommand::MoveToLineStart { select: false }),
                insert("MACRO"),
            ])
        );

        // Esc fires its own meaning at once (see `lone_escape_fires_at_once...`) and stays
        // held as a meta prefix, but nothing binds \e\C-r here, so the pair is not a key:
        // the escape keeps its meaning and \C-r is dispatched afresh, firing the macro.
        // (bash does bind \e\C-r, to revert-line, so there the pair is a key.)
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(press_ctrl(&mut bindings, 'r')?, insert("atuin"));

        Ok(())
    }

    #[test]
    fn a_sequence_can_start_at_the_key_that_missed() -> Result<(), std::io::Error> {
        // \C-a\C-e and \C-t\C-e are bound; typing \C-a \C-t \C-e runs \C-a alone and
        // then the \C-t\C-e binding, as in readline.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x01\x05".to_vec()),
            KeyAction::ShellCommand("ae".to_owned()),
        )?;
        bindings.bind(
            KeySequence::from(b"\x14\x05".to_vec()),
            KeyAction::ShellCommand("te".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'a')?, ReedlineEvent::None);
        assert_eq!(
            press_ctrl(&mut bindings, 't')?,
            edit(EditCommand::MoveToLineStart { select: false })
        );
        assert_eq!(
            press_ctrl(&mut bindings, 'e')?,
            ReedlineEvent::ExecuteHostCommand("te".to_owned())
        );

        Ok(())
    }

    #[test]
    fn the_longest_bound_prefix_of_held_keys_fires() -> Result<(), std::io::Error> {
        // \C-x\C-r is a macro key and \C-x\C-r\C-e a raw binding: on \C-x \C-r z the
        // two-key macro fires, not \C-x alone, and z is dispatched after it.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x12\x05".to_vec()),
            KeyAction::ShellCommand("xre".to_owned()),
        )?;
        define(
            &mut bindings,
            KeySequence::from(b"\x18\x12".to_vec()),
            b"xr",
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(press_ctrl(&mut bindings, 'r')?, ReedlineEvent::None);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('z'))?,
            ReedlineEvent::Multiple(vec![insert("xr"), edit(EditCommand::InsertChar('z'))])
        );

        Ok(())
    }

    #[test]
    fn a_run_of_escapes_binds_separately_from_a_meta_key() -> Result<(), std::io::Error> {
        // \e\ef is two keys (see `keys::a_run_of_escapes_is_not_one_key`), so it and \M-f
        // are distinct bindings.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x1bf".to_vec()),
            KeyAction::DoInputFunction(InputFunction::ForwardWord),
        )?;
        bindings.bind(
            KeySequence::from(b"\x1b\x1bf".to_vec()),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;
        assert_eq!(
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Char('f'))?,
            edit(EditCommand::MoveWordRight { select: false })
        );
        let current = bindings.get_current();
        assert_eq!(
            current.get(&KeySequence::from(b"\x1bf".to_vec())),
            Some(&KeyAction::DoInputFunction(InputFunction::ForwardWord))
        );
        assert_eq!(
            current.get(&KeySequence::from(b"\x1b\x1bf".to_vec())),
            Some(&KeyAction::DoInputFunction(InputFunction::ClearScreen))
        );

        Ok(())
    }

    #[test]
    fn raw_sequence_fires_however_its_keys_are_spelled() -> Result<(), std::io::Error> {
        // A raw sequence is canonicalized key by key, so a binding written with the SS3 up
        // arrow fires when the terminal reports the key, lists in canonical form, and can be
        // unbound by either spelling.
        let mut bindings = new_bindings();
        let as_written = KeySequence::from(b"\x18\x1bOA".to_vec());
        let canonical_form = KeySequence::from(b"\x18\x1b[A".to_vec());
        bindings.bind(
            as_written.clone(),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Up)?,
            ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );
        let current = bindings.get_current();
        assert_eq!(
            current.get(&canonical_form),
            Some(&KeyAction::ShellCommand("raw".to_owned()))
        );
        assert!(!current.contains_key(&as_written));

        // The same for a macro key, and for the bytes replayed by a macro body.
        define(&mut bindings, as_written.clone(), b"text")?;
        assert!(bindings.get_macros().contains_key(&canonical_form));
        assert!(!bindings.get_current().contains_key(&canonical_form));
        define(&mut bindings, control_key('g'), b"\x18\x1bOA")?;
        assert_eq!(press_ctrl(&mut bindings, 'g')?, insert("text"));

        assert!(bindings.try_unbind(as_written));
        assert!(!bindings.get_macros().contains_key(&canonical_form));
        assert!(!bindings.try_unbind(canonical_form));

        Ok(())
    }

    #[test]
    fn shift_tab_binds_and_fires() -> Result<(), std::io::Error> {
        // The terminal reports \e[Z as shift+back-tab, which is also how reedline binds it.
        let mut bindings = new_bindings();
        let seq = KeySequence::from(b"\x1b[Z".to_vec());
        bindings.bind(
            seq.clone(),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::SHIFT, KeyCode::BackTab)?,
            ReedlineEvent::ClearScreen
        );
        assert_eq!(
            bindings.get_current().get(&seq),
            Some(&KeyAction::DoInputFunction(InputFunction::ClearScreen))
        );
        assert!(bindings.try_unbind(seq));
        assert_ne!(
            press(&mut bindings, KeyModifiers::SHIFT, KeyCode::BackTab)?,
            ReedlineEvent::ClearScreen
        );

        Ok(())
    }

    #[test]
    fn modified_function_keys_bind_and_fire() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        let seq = KeySequence::from(b"\x1b[1;5P".to_vec());
        bindings.bind(
            seq.clone(),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::CONTROL, KeyCode::F(1))?,
            ReedlineEvent::ClearScreen
        );
        assert!(bindings.get_current().contains_key(&seq));

        Ok(())
    }

    #[test]
    fn uppercase_function_bindings_fire_from_the_keyboard() -> Result<(), std::io::Error> {
        // Terminals report a capital with SHIFT set; the binding is on the character.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"A".to_vec()),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;
        bindings.bind(
            alt_key('F'),
            KeyAction::DoInputFunction(InputFunction::KillWord),
        )?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::SHIFT, KeyCode::Char('A'))?,
            ReedlineEvent::ClearScreen
        );
        assert_eq!(
            press(
                &mut bindings,
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                KeyCode::Char('F')
            )?,
            edit(EditCommand::CutWordRight)
        );

        Ok(())
    }

    #[test]
    fn a_meta_key_resolves_the_same_way_pressed_as_replayed() -> Result<(), std::io::Error> {
        // readline binds \M-A..\M-Z to do-lowercase-version, so a meta upper-case key falls
        // back to its lower-case binding. That has to hold from the keyboard exactly as it
        // does inside a macro body, whether the lower-case key holds a function or a macro.
        let mut bindings = new_bindings();
        bindings.bind(
            alt_key('f'),
            KeyAction::DoInputFunction(InputFunction::KillWord),
        )?;
        define(&mut bindings, alt_key('c'), b"hi")?;
        define(&mut bindings, control_key('g'), b"\x1bF\x1bC")?;

        let replayed = press_ctrl(&mut bindings, 'g')?;
        let pressed = ReedlineEvent::Multiple(vec![
            press(
                &mut bindings,
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                KeyCode::Char('F'),
            )?,
            press(
                &mut bindings,
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                KeyCode::Char('C'),
            )?,
        ]);

        assert_eq!(replayed, pressed);
        assert_eq!(
            replayed,
            ReedlineEvent::Multiple(vec![edit(EditCommand::CutWordRight), insert("hi")])
        );

        Ok(())
    }

    #[test]
    fn lone_escape_fires_at_once_and_still_completes_a_meta_macro_key() -> Result<(), std::io::Error>
    {
        // fzf binds \ec as a macro. The terminal reports Alt+c as one key, which fires it.
        // A lone Esc is a prefix of that key's bytes, so it is held; but with no
        // key-sequence timeout that would leave a completion menu open, so its meaning
        // (which only resets editor state) fires right away, and the miss that follows
        // does not repeat it.
        let mut bindings = new_bindings();
        define(&mut bindings, alt_key('c'), b"macro")?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Char('c'))?,
            insert("macro")
        );

        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        // The escape stays held, so the key after it completes a meta key: \eb is
        // backward-word, not an escape followed by a `b` (checked against bash).
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('b'))?,
            edit(EditCommand::MoveWordLeft { select: false })
        );

        // Still held: Esc followed by c completes the macro key, as it does in readline.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('c'))?,
            insert("macro")
        );

        // Esc Esc: the first has fired and is not repeated; the second fires and is held.
        // Held here because \ec still needs it, so the key after it is a meta key. bash
        // instead reads \e\e as one unbound sequence and leaves the `b` as text; see
        // `escapes_pair_off_before_prefixing_the_next_key` for the case with nothing else
        // holding the escape, where they do pair off as they do in bash.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('b'))?,
            edit(EditCommand::MoveWordLeft { select: false })
        );

        Ok(())
    }

    #[test]
    fn escapes_pair_off_before_prefixing_the_next_key() -> Result<(), std::io::Error> {
        // The terminal reports a run of escapes as separate keys, and they pair off: an
        // escape released by the miss of the one before it is not a meta prefix in turn, so
        // whether the key after a run is a meta key depends on the run's parity. bash pairs
        // them the same way, by reading `\e\e` as one sequence (it binds that sequence to
        // `complete`; brush has nothing on it, so each escape only takes its own meaning).
        let mut bindings = new_bindings();

        // Two escapes pair off, leaving the key after them ordinary text.
        for _ in 0..2 {
            assert_eq!(
                press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
                ReedlineEvent::Esc
            );
        }
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('b'))?,
            edit(EditCommand::InsertChar('b'))
        );

        // Three: the first two pair off and the third prefixes the key after it.
        for _ in 0..3 {
            assert_eq!(
                press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
                ReedlineEvent::Esc
            );
        }
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('b'))?,
            edit(EditCommand::MoveWordLeft { select: false })
        );

        // One escape on its own still prefixes what follows it.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('b'))?,
            edit(EditCommand::MoveWordLeft { select: false })
        );

        Ok(())
    }

    #[test]
    fn a_replayed_escape_does_not_release_the_next_pressed_one() -> Result<(), std::io::Error> {
        // Pairing off is about a run of escapes *the user pressed*: an escape a macro body
        // replayed is resolved in its own pass and has nothing to do with the escape the
        // user presses next, which must still prefix the key after it.
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"\x1b")?;

        assert_eq!(press_ctrl(&mut bindings, 'g')?, ReedlineEvent::Esc);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('f'))?,
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Char('f'))?
        );

        Ok(())
    }

    #[test]
    fn a_held_escape_makes_the_next_key_a_meta_key() -> Result<(), std::io::Error> {
        // readline's meta prefix: the terminal reports Esc and the key after it as two
        // events, and the pair is one meta key. All checked against bash.
        let mut bindings = new_bindings();

        // Bound: \ef is forward-word, whether it arrives as Alt+f or as Esc then f.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('f'))?,
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Char('f'))?
        );

        // Unbound: the pair is not a key, so the escape keeps its own meaning and the key
        // after it is dispatched afresh. bash instead rings the bell and drops both; this
        // is the policy a missed prefix already follows, and it is what keeps Esc usable
        // as the gesture that dismisses a menu without swallowing the next keystroke.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('q'))?,
            edit(EditCommand::InsertChar('q'))
        );
        // A bound command reached the same way still runs, rather than being eaten.
        bindings.bind(control_key('t'), KeyAction::ShellCommand("ran".to_owned()))?;
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(press_ctrl(&mut bindings, 't')?, host_event("ran"));

        // Esc then Enter is \e\r, whatever that is bound to -- never the bare accept-line
        // that Enter alone would be. bash does not accept the line here either.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        let accept = base_binding(&bindings, KeyCode::Enter)?;
        let escaped_enter = press(&mut bindings, KeyModifiers::NONE, KeyCode::Enter)?;
        assert_ne!(escaped_enter, accept);
        assert_eq!(
            escaped_enter,
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Enter)?
        );

        // A key the escape cannot combine with releases it: the terminal sends a cursor
        // key as its own escape sequence, so Esc then Up is two keys, not a meta key.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Up)?,
            base_binding(&bindings, KeyCode::Up)?
        );

        Ok(())
    }

    #[test]
    fn a_held_escape_is_dropped_when_the_read_ends() -> Result<(), std::io::Error> {
        // Nothing may carry an escape into the next line: the key after the prompt is its
        // own key, not the tail of a meta key from the line before.
        let mut bindings = new_bindings();

        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::Esc
        );
        assert!(bindings.end_read(None).is_none());
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('f'))?,
            edit(EditCommand::InsertChar('f'))
        );

        Ok(())
    }

    #[test]
    fn a_replayed_escape_does_not_wait_for_a_keystroke() -> Result<(), std::io::Error> {
        // Only a pressed escape is held for the key after it. One at the end of a macro
        // body has nothing more coming in its own stream, so it resolves right away
        // rather than stalling the body on a keystroke that may never arrive.
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"ab\x1b")?;

        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            ReedlineEvent::Multiple(vec![insert("ab"), ReedlineEvent::Esc])
        );

        // And a macro body spells a meta key itself, without waiting for anything.
        define(&mut bindings, control_key('g'), b"\x1bf")?;
        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            press(&mut bindings, KeyModifiers::ALT, KeyCode::Char('f'))?
        );

        Ok(())
    }

    #[test]
    fn escape_rebound_away_from_its_default_waits_like_any_prefix() -> Result<(), std::io::Error> {
        // Firing early is only safe for the default escape meaning. Bound to something
        // else, Esc waits, and a miss releases it ahead of the key that missed.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x1b".to_vec()),
            KeyAction::DoInputFunction(InputFunction::ClearScreen),
        )?;
        define(&mut bindings, alt_key('c'), b"macro")?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::None
        );
        // A longer key still wins over the escape's own binding: \eb is backward-word,
        // and the clear-screen bound to \e never fires (checked against bash).
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('b'))?,
            edit(EditCommand::MoveWordLeft { select: false })
        );

        // A key the escape cannot combine with does release it, ahead of that key.
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::None
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Up)?,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::ClearScreen,
                base_binding(&bindings, KeyCode::Up)?,
            ])
        );

        Ok(())
    }

    #[test]
    fn escape_in_a_longer_held_sequence_is_not_fired_early() -> Result<(), std::io::Error> {
        // The early fire is for a lone escape only: \C-x then Esc, held for \C-x\ec, waits.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x1bc".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Esc)?,
            ReedlineEvent::None
        );
        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('c'))?,
            ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );

        Ok(())
    }

    #[test]
    fn held_keys_are_dropped_when_a_read_ends() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        bindings.end_read(None);

        // The prefix did not survive into the next read.
        assert_ne!(
            press_ctrl(&mut bindings, 'r')?,
            ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );

        Ok(())
    }

    #[test]
    fn non_key_events_do_not_release_a_held_prefix() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x18\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'x')?, ReedlineEvent::None);
        assert_eq!(
            send(&mut bindings, crossterm::event::Event::Resize(80, 24))?,
            ReedlineEvent::Resize(80, 24)
        );
        assert_eq!(
            press_ctrl(&mut bindings, 'r')?,
            ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );

        Ok(())
    }

    #[test]
    fn interrupt_releases_a_held_prefix() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\x01\x12".to_vec()),
            KeyAction::ShellCommand("raw".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 'a')?, ReedlineEvent::None);
        assert_eq!(
            press_ctrl(&mut bindings, 'c')?,
            ReedlineEvent::Multiple(vec![
                edit(EditCommand::MoveToLineStart { select: false }),
                ReedlineEvent::CtrlC,
            ])
        );

        Ok(())
    }

    #[test]
    fn text_after_accept_line_in_a_macro_is_deferred() -> Result<(), std::io::Error> {
        // bash runs both lines; reedline returns on the first accept, so the rest is held
        // back the same way it is behind a bound command.
        let mut bindings = new_bindings();
        let body = b"echo a\recho b\r";
        define(&mut bindings, control_key('g'), body)?;

        let event = press_ctrl(&mut bindings, 'g')?;
        assert!(
            matches!(&event, ReedlineEvent::Multiple(events)
                if events.len() == 2 && events[0] == insert("echo a")),
            "unexpected event: {event:?}"
        );
        assert_eq!(
            deferred(&mut bindings, &event)?,
            Deferred {
                action: DeferredAction::AcceptLine,
                replay: Some(replay(b"echo b\r", body.len())),
            }
        );

        Ok(())
    }

    #[test]
    fn accept_line_inside_nested_macro_defers_outer_remainder() -> Result<(), std::io::Error> {
        // The inner macro ends in accept-line and the outer has more after it: bash runs
        // both lines, so the outer remainder must be deferred just as it is behind a bound
        // command, not emitted after an Enter reedline will return on.
        let mut bindings = new_bindings();
        let inner = b"echo a\r";
        let outer = b"\x14echo b\r";
        define(&mut bindings, control_key('t'), inner)?;
        define(&mut bindings, control_key('g'), outer)?;

        let event = press_ctrl(&mut bindings, 'g')?;
        assert!(
            matches!(&event, ReedlineEvent::Multiple(events)
                if events.len() == 2 && events[0] == insert("echo a")),
            "unexpected event: {event:?}"
        );
        assert_eq!(
            deferred(&mut bindings, &event)?,
            Deferred {
                action: DeferredAction::AcceptLine,
                replay: Some(replay(b"echo b\r", inner.len() + outer.len())),
            }
        );

        Ok(())
    }

    #[test]
    fn accept_line_at_the_end_of_a_macro_stays_an_enter() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"echo a\r")?;

        assert_eq!(
            press_ctrl(&mut bindings, 'g')?,
            ReedlineEvent::Multiple(vec![insert("echo a"), ReedlineEvent::Enter])
        );

        Ok(())
    }

    #[test]
    fn tab_can_be_rebound() -> Result<(), std::io::Error> {
        // Issue #795: `bind '"\t": brush-accept-hint'`.
        let mut bindings = new_bindings();
        bindings.bind(
            KeySequence::from(b"\t".to_vec()),
            KeyAction::DoInputFunction(InputFunction::BrushAcceptHint),
        )?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Tab)?,
            ReedlineEvent::HistoryHintComplete
        );
        assert_eq!(
            bindings
                .get_current()
                .get(&KeySequence::from(b"\t".to_vec())),
            Some(&KeyAction::DoInputFunction(InputFunction::BrushAcceptHint))
        );

        Ok(())
    }

    #[test]
    fn binding_an_unsupported_function_names_it() {
        // Issue #1273: the failure used to surface as a bare "I/O error occurred".
        let mut bindings = new_bindings();
        let result = bindings.bind(
            control_key('g'),
            KeyAction::DoInputFunction(InputFunction::HistorySearchForward),
        );

        assert!(
            matches!(&result, Err(err) if err.to_string().contains("history-search-forward")),
            "expected an error naming the function, got {result:?}"
        );
    }

    #[test]
    fn unbind_removes_byte_macro() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        let seq = KeySequence::from(b"xy".to_vec());
        define(&mut bindings, seq.clone(), b"")?;

        assert!(bindings.try_unbind(seq.clone()));
        assert!(!bindings.try_unbind(seq));
        Ok(())
    }

    #[test]
    fn get_current_is_stable_when_two_keys_spell_the_same_sequence() {
        // The terminal reports `A` as Shift+`A`, so a binding on Shift+`a` would spell the
        // same `a` as an unmodified one. reedline's map has no defined iteration order, so
        // the listing must not depend on it: fewest modifiers wins, every time.
        let mut bindings = new_bindings();
        bindings.update(|map| {
            map.add_binding(
                KeyModifiers::NONE,
                KeyCode::Char('a'),
                edit(EditCommand::Undo),
            );
            map.add_binding(
                KeyModifiers::SHIFT,
                KeyCode::Char('a'),
                edit(EditCommand::Redo),
            );
        });

        let seq = KeySequence::from(b"a".to_vec());
        let expected = Some(KeyAction::DoInputFunction(InputFunction::Undo));
        for _ in 0..64 {
            assert_eq!(bindings.get_current().get(&seq).cloned(), expected);
        }
    }

    #[test]
    fn a_stop_nested_in_a_bound_event_defers_the_rest() -> Result<(), std::io::Error> {
        // A composite event can hide the stop inside it. Whether resolution stops has to be
        // decided on the same flattened view the deferral itself walks, or the tail would be
        // handed to reedline after a host command and silently dropped.
        for tail in [reedline::ReedlineEvent::Enter, host_event("nested")] {
            let mut bindings = new_bindings();
            bindings.update(|map| {
                map.add_binding(
                    KeyModifiers::CONTROL,
                    KeyCode::Char('y'),
                    ReedlineEvent::Multiple(vec![
                        edit(EditCommand::Undo),
                        ReedlineEvent::Multiple(vec![tail.clone()]),
                    ]),
                );
            });
            define(&mut bindings, control_key('g'), b"\x19rest")?;

            let event = press_ctrl(&mut bindings, 'g')?;
            let stopped = deferred(&mut bindings, &event)?;
            assert_eq!(
                stopped.replay.map(|replay| replay.bytes()),
                Some(b"rest".to_vec()),
                "{tail:?}"
            );
        }

        Ok(())
    }

    #[test]
    fn a_bare_stop_from_the_keyboard_is_not_deferred() -> Result<(), std::io::Error> {
        // The other side of the same guard: with nothing pending, an ordinary keyboard
        // press of a bound command goes straight back to reedline.
        let mut bindings = new_bindings();
        bindings.bind(
            control_key('t'),
            KeyAction::ShellCommand("plain".to_owned()),
        )?;

        assert_eq!(press_ctrl(&mut bindings, 't')?, host_event("plain"));

        Ok(())
    }

    #[test]
    fn a_chunk_with_no_bytes_and_no_event_is_dropped() {
        // Unreachable through the public surface (every chunk has bytes or a native
        // event), so this pins that resolution still terminates rather than matching on
        // nothing.
        let mut bindings = new_bindings();
        bindings.pending.push_back(PendingInput {
            bytes: VecDeque::new(),
            event: None,
            fired: false,
        });

        assert_eq!(bindings.resolve_pending(), ReedlineEvent::None);
        assert!(bindings.pending.is_empty());
    }

    #[test]
    fn an_action_and_a_macro_never_share_a_key() -> Result<(), std::io::Error> {
        // One entry per key in one store is what makes this hold; check both directions and
        // both key shapes (a single key, which reedline dispatches, and a longer sequence).
        for seq in [control_key('t'), KeySequence::from(b"\x18\x12".to_vec())] {
            let mut bindings = new_bindings();

            bindings.bind(seq.clone(), KeyAction::ShellCommand("cmd".to_owned()))?;
            define(&mut bindings, seq.clone(), b"body")?;
            assert!(!bindings.get_current().contains_key(&seq), "{seq}");
            assert!(bindings.get_macros().contains_key(&seq), "{seq}");

            bindings.bind(seq.clone(), KeyAction::ShellCommand("cmd".to_owned()))?;
            assert!(bindings.get_current().contains_key(&seq), "{seq}");
            assert!(!bindings.get_macros().contains_key(&seq), "{seq}");

            assert!(bindings.try_unbind(seq.clone()), "{seq}");
            assert!(!bindings.get_current().contains_key(&seq), "{seq}");
            assert!(!bindings.get_macros().contains_key(&seq), "{seq}");
            assert!(!bindings.try_unbind(seq.clone()), "{seq}");
        }

        Ok(())
    }

    #[test]
    fn non_macro_keys_pass_through_to_reedline() -> Result<(), std::io::Error> {
        let mut bindings = new_bindings();
        define(&mut bindings, control_key('g'), b"text")?;

        assert_eq!(
            press(&mut bindings, KeyModifiers::NONE, KeyCode::Char('q'))?,
            edit(EditCommand::InsertChar('q'))
        );
        assert_eq!(
            press_ctrl(&mut bindings, 'a')?,
            edit(EditCommand::MoveToLineStart { select: false })
        );
        Ok(())
    }
}
