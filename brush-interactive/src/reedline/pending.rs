//! The pending input stream: unconsumed terminal keys and macro bytes, in one sequence.
//!
//! Keyboard input and macro bodies share one stream, so a trigger, terminal escape sequence
//! or UTF-8 character can span the boundary between a macro body and the keys after it.
//! Nothing here knows about bindings; the rules it enforces are about the *stream*: what
//! bytes are available to match, where a native terminal event may not be split, and what a
//! barrier is.

use std::collections::VecDeque;

use super::keys::{MAX_KEY_LEN, ReedlineKey, decode_key, key_sequence_for};

/// A contiguous part of the input stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingInput {
    pub bytes: VecDeque<u8>,
    /// Native keys retain their exact modifiers and ordinary editor behavior. An empty byte
    /// spelling is a barrier to sequence matching, not an empty trigger.
    pub event: Option<crossterm::event::Event>,
    /// Whether its ordinary meaning already fired while held (see
    /// `UpdatableBindings::fire_lone_escape_early`), so a miss must not repeat it.
    pub fired: bool,
}

impl PendingInput {
    /// A chunk of macro bytes: nothing native behind it, nothing fired yet.
    pub fn macro_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec().into(),
            event: None,
            fired: false,
        }
    }

    /// A terminal event, with the bytes it is spelled with -- none when spelling it would
    /// lose its meaning, which makes it a barrier.
    pub const fn native(event: crossterm::event::Event, bytes: VecDeque<u8>) -> Self {
        Self {
            bytes,
            event: Some(event),
            fired: false,
        }
    }
}

/// The pending input stream. Everything that changes it goes through a method here, so the
/// rules above hold; [`Deref`](std::ops::Deref) exposes the read-only deque view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Pending(VecDeque<PendingInput>);

impl std::ops::Deref for Pending {
    type Target = VecDeque<PendingInput>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Pending {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Puts `rest` back behind whatever is already held, as a resumed replay does.
    pub fn append(&mut self, rest: Self) {
        self.0.extend(rest.0);
    }

    pub fn push_back(&mut self, input: PendingInput) {
        self.0.push_back(input);
    }

    pub fn push_front(&mut self, input: PendingInput) {
        self.0.push_front(input);
    }

    /// The front chunk, to mark its ordinary meaning as already fired.
    pub fn front_mut(&mut self) -> Option<&mut PendingInput> {
        self.0.front_mut()
    }

    /// Drops the front chunk outright. Only for a chunk carrying nothing to match.
    pub fn pop_front(&mut self) -> Option<PendingInput> {
        self.0.pop_front()
    }

    /// The bytes available for matching -- everything up to the first barrier, capped at
    /// `limit` -- and whether a barrier cut them short. A barrier within the window means
    /// the input after a held prefix has already arrived, so waiting for more is pointless.
    pub fn matchable(&self, limit: usize) -> (Vec<u8>, bool) {
        let mut bytes = Vec::new();
        for input in &self.0 {
            if input.bytes.is_empty() {
                return (bytes, true);
            }
            bytes.extend(input.bytes.iter().copied());
            if bytes.len() >= limit {
                bytes.truncate(limit);
                return (bytes, false);
            }
        }

        (bytes, false)
    }

    /// The key `bytes` starts with, and how many bytes it takes: the longest prefix the
    /// terminal would report as one key, except one that could only match by splitting a
    /// native terminal event. The one place that rule is applied, so [`Self::normalize`]
    /// and the resolver agree on where a key ends.
    pub fn leading_key(&self, bytes: &[u8]) -> Option<(ReedlineKey, usize)> {
        decode_key(bytes).filter(|(_, len)| self.is_boundary(*len))
    }

    /// Canonicalizes the matching window only, leaving the suffix in place, so splicing a
    /// child macro does not re-normalize the outer body. Returns how many input bytes were
    /// rewritten; only [`tests::normalization_work_is_bounded_by_the_window`] reads that.
    pub fn normalize(&mut self, limit: usize) -> usize {
        let mut normalized: VecDeque<PendingInput> = VecDeque::new();
        let mut normalized_len = 0;
        let mut examined = 0;
        while let Some(first) = self.0.front() {
            if normalized_len >= limit || first.bytes.is_empty() {
                break;
            }
            if first.event.is_some() {
                if let Some(input) = self.0.pop_front() {
                    normalized_len += input.bytes.len();
                    examined += input.bytes.len();
                    normalized.push_back(input);
                }
                continue;
            }
            let (bytes, _) = self.matchable(MAX_KEY_LEN);
            let (spelled, consumed) = self
                .leading_key(&bytes)
                .and_then(|((modifiers, code), len)| {
                    Some((key_sequence_for(modifiers, code)?.into_bytes(), len))
                })
                .unwrap_or_else(|| (vec![bytes[0]], 1));
            examined += consumed;
            normalized_len += spelled.len();
            self.consume(consumed);
            if let Some(previous) = normalized.back_mut()
                && previous.event.is_none()
            {
                previous.bytes.extend(spelled);
            } else {
                normalized.push_back(PendingInput::macro_bytes(&spelled));
            }
        }
        for input in normalized.into_iter().rev() {
            if input.event.is_none()
                && let Some(next) = self.0.front_mut()
                && next.event.is_none()
            {
                for byte in input.bytes.into_iter().rev() {
                    next.bytes.push_front(byte);
                }
            } else {
                self.0.push_front(input);
            }
        }

        examined
    }

    /// Consumes a matched byte prefix. A count of zero drops the chunk at the front, which
    /// must be a barrier (a native event with no byte spelling); a resolution step reports
    /// zero only for such a chunk.
    pub fn consume(&mut self, mut count: usize) {
        if count == 0 {
            debug_assert!(
                self.0.front().is_none_or(|input| input.bytes.is_empty()),
                "consuming no bytes would drop a chunk that still has some"
            );
            self.0.pop_front();
            return;
        }

        while let Some(mut input) = self.0.pop_front() {
            if count < input.bytes.len() {
                input.bytes.drain(..count);
                self.0.push_front(input);
                return;
            }
            count -= input.bytes.len();
            if count == 0 {
                return;
            }
        }
    }

    /// Sequence matching must not split a terminal event (for example, consume only the
    /// escape byte from an Alt key). Macro bytes have no such boundary.
    pub fn is_boundary(&self, mut count: usize) -> bool {
        for input in &self.0 {
            if count < input.bytes.len() {
                return input.event.is_none();
            }
            count -= input.bytes.len();
            if count == 0 {
                return true;
            }
        }
        false
    }
}

/// Unconsumed input held behind a host command or accept-line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeferredReplay {
    pub pending: Pending,
    /// What is left of the budget of the keystroke this input came from, so that a macro
    /// which reaches itself through a bound command still runs out.
    pub budget: usize,
}

#[cfg(test)]
impl DeferredReplay {
    pub fn new(bytes: &[u8], budget: usize) -> Self {
        let mut pending = Pending::default();
        if !bytes.is_empty() {
            pending.push_back(PendingInput::macro_bytes(bytes));
        }

        Self { pending, budget }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.pending
            .iter()
            .flat_map(|input| input.bytes.iter().copied())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_work_is_bounded_by_the_window() {
        // Each pass rewrites the matching window and no more, whatever the length of the
        // suffix behind it. That is what keeps splicing a child macro into a long outer
        // body linear rather than quadratic: the outer suffix is not rebuilt every time.
        for suffix in [0, 1, 64, 4096, 65536] {
            for limit in [MAX_KEY_LEN, 32] {
                let mut pending = Pending::default();
                pending.push_back(PendingInput::macro_bytes(
                    &std::iter::repeat_n(b'a', suffix + limit).collect::<Vec<_>>(),
                ));
                let examined = pending.normalize(limit);
                assert!(
                    examined <= limit + MAX_KEY_LEN,
                    "suffix {suffix}, limit {limit}: normalized {examined} bytes"
                );
                assert_eq!(
                    pending.matchable(usize::MAX).0.len(),
                    suffix + limit,
                    "normalizing changed the stream's length"
                );
            }
        }
    }

    #[test]
    fn consuming_no_bytes_only_ever_drops_a_barrier() {
        // Zero is how a barrier -- a native event with no byte spelling -- is consumed,
        // and it must not reach past that one chunk.
        let barrier = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            reedline::KeyCode::Backspace,
            reedline::KeyModifiers::CONTROL,
        ));
        let mut pending = Pending::default();
        pending.push_back(PendingInput::native(barrier, VecDeque::new()));
        pending.push_back(PendingInput::macro_bytes(b"ab"));

        pending.consume(0);
        assert_eq!(pending.matchable(usize::MAX).0, b"ab");
        assert_eq!(pending.len(), 1);

        // And an empty stream tolerates it.
        let mut empty = Pending::default();
        empty.consume(0);
        assert!(empty.is_empty());
    }

    #[test]
    fn consuming_bytes_spans_chunks_and_splits_the_one_it_stops_in() {
        let mut pending = Pending::default();
        pending.push_back(PendingInput::macro_bytes(b"ab"));
        pending.push_back(PendingInput::macro_bytes(b"cde"));

        // Exactly one chunk: the chunk goes, the next stays whole.
        pending.consume(2);
        assert_eq!(pending.matchable(usize::MAX).0, b"cde");
        assert_eq!(pending.len(), 1);

        // Part of a chunk: the rest stays.
        pending.consume(1);
        assert_eq!(pending.matchable(usize::MAX).0, b"de");

        // More than is there: the stream empties rather than wrapping around.
        pending.consume(9);
        assert!(pending.is_empty());
    }

    #[test]
    fn a_key_is_never_split_across_a_native_event() {
        // The rule `leading_key` exists to hold: Alt+f arrives as one event spelled
        // `ESC f`, so a macro's trailing escape must not pair with the `f` behind it.
        let mut pending = Pending::default();
        pending.push_back(PendingInput::macro_bytes(b"\x1b"));
        pending.push_back(PendingInput::native(
            crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                reedline::KeyCode::Char('f'),
                reedline::KeyModifiers::ALT,
            )),
            b"\x1bf".to_vec().into(),
        ));

        let (bytes, _) = pending.matchable(MAX_KEY_LEN);
        assert_eq!(bytes, b"\x1b\x1bf");
        // Two bytes would split the native event; one byte is the lone escape.
        assert!(!pending.is_boundary(2));
        assert_eq!(
            pending.leading_key(&bytes),
            Some(((reedline::KeyModifiers::NONE, reedline::KeyCode::Esc), 1))
        );
    }
}
