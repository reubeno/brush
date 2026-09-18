//! The seam between brush's key actions and reedline's events.
//!
//! Two directions and one wire format: [`translate_action_to_reedline_event`] turns a bound
//! action into the event reedline dispatches, [`translate_reedline_event_to_action`] turns a
//! reedline event back into the readline function `bind` lists it as, and [`HostCommand`]
//! is what travels through reedline's string-only host interface. Nothing here knows about
//! the pending input stream.

use brush_core::{
    interfaces::{InputFunction, KeyAction},
    trace_categories,
};
use std::collections::VecDeque;

#[derive(thiserror::Error, Debug)]
pub(super) enum KeyError {
    /// Unsupported key action
    #[error("unsupported key action: {0}")]
    UnsupportedKeyAction(KeyAction),
}

/// What the string reedline hands back for a bound key stands for.
///
/// reedline returns nothing but a string for a bound key, so our own meanings travel behind
/// a leading NUL. A `bind -x` command comes from a shell word, which cannot start with a NUL,
/// so nothing else is ever read as a marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HostCommand {
    /// A shell command bound with `bind -x`.
    Command(String),
    /// A point at which macro resolution stopped: the index of the `Deferred` record the
    /// editor holds for it.
    Deferred(usize),
    /// A readline function the editor cannot carry out on its own because it needs the
    /// shell (`shell-expand-line`).
    InputFunction(InputFunction),
}

impl HostCommand {
    const INPUT_FUNCTION: &str = "\0function ";
    const DEFERRED: &str = "\0deferred ";

    pub fn decode(host_command: &str) -> Self {
        if let Some(function) = host_command
            .strip_prefix(Self::INPUT_FUNCTION)
            .and_then(|name| name.parse().ok())
        {
            Self::InputFunction(function)
        } else if let Some(id) = host_command
            .strip_prefix(Self::DEFERRED)
            .and_then(|id| id.parse().ok())
        {
            Self::Deferred(id)
        } else {
            Self::Command(host_command.to_owned())
        }
    }

    pub fn encode(&self) -> String {
        match self {
            Self::Command(command) => command.clone(),
            Self::Deferred(id) => std::format!("{}{id}", Self::DEFERRED),
            Self::InputFunction(function) => std::format!("{}{function}", Self::INPUT_FUNCTION),
        }
    }
}

/// What stopped a macro resolution: both make reedline return to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DeferredAction {
    /// A bound host command to run.
    RunCommand(HostCommand),
    /// An accept-line with more macro bytes after it.
    AcceptLine,
}

/// The events `event` is made of, in order: nested `Multiple`s are flattened and `None`s
/// dropped.
pub(super) fn flatten(event: reedline::ReedlineEvent) -> Vec<reedline::ReedlineEvent> {
    let mut flat = Vec::new();
    let mut queue = VecDeque::from([event]);

    while let Some(event) = queue.pop_front() {
        match event {
            reedline::ReedlineEvent::Multiple(events) => {
                for event in events.into_iter().rev() {
                    queue.push_front(event);
                }
            }
            reedline::ReedlineEvent::None => {}
            other => flat.push(other),
        }
    }

    flat
}

/// One event for `events`: none, the one, or a `Multiple` of those that aren't `None`.
pub(super) fn combine(events: Vec<reedline::ReedlineEvent>) -> reedline::ReedlineEvent {
    let mut events: Vec<_> = events
        .into_iter()
        .filter(|event| *event != reedline::ReedlineEvent::None)
        .collect();

    match events.len() {
        0 => reedline::ReedlineEvent::None,
        1 => events.pop().unwrap_or(reedline::ReedlineEvent::None),
        _ => reedline::ReedlineEvent::Multiple(events),
    }
}

pub(super) fn translate_action_to_reedline_event(
    action: &KeyAction,
) -> Option<reedline::ReedlineEvent> {
    match action {
        KeyAction::ShellCommand(cmd) => Some(reedline::ReedlineEvent::ExecuteHostCommand(
            HostCommand::Command(cmd.to_owned()).encode(),
        )),
        KeyAction::DoInputFunction(func) => translate_input_function_to_reedline_event(func),
    }
}

fn translate_input_function_to_reedline_event(
    func: &InputFunction,
) -> Option<reedline::ReedlineEvent> {
    use reedline::{EditCommand, ReedlineEvent};

    match func {
        InputFunction::BackwardDeleteChar => {
            Some(ReedlineEvent::Edit(vec![EditCommand::Backspace]))
        }
        InputFunction::BackwardKillWord => {
            Some(ReedlineEvent::Edit(vec![EditCommand::CutWordLeft]))
        }
        InputFunction::KillLine => Some(ReedlineEvent::Edit(vec![EditCommand::KillLine])),
        InputFunction::KillWholeLine => Some(ReedlineEvent::Edit(vec![EditCommand::CutFromStart])),
        InputFunction::KillWord => Some(ReedlineEvent::Edit(vec![EditCommand::CutWordRight])),
        InputFunction::DeleteChar => Some(ReedlineEvent::Edit(vec![EditCommand::Delete])),
        InputFunction::DowncaseWord => Some(ReedlineEvent::Edit(vec![EditCommand::LowercaseWord])),
        InputFunction::BackwardChar => Some(ReedlineEvent::Edit(vec![EditCommand::MoveLeft {
            select: false,
        }])),
        InputFunction::ForwardChar => Some(ReedlineEvent::Edit(vec![EditCommand::MoveRight {
            select: false,
        }])),
        InputFunction::EndOfLine => Some(ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd {
            select: false,
        }])),
        InputFunction::BeginningOfLine => {
            Some(ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart {
                select: false,
            }]))
        }
        InputFunction::BackwardWord => Some(ReedlineEvent::Edit(vec![EditCommand::MoveWordLeft {
            select: false,
        }])),
        InputFunction::ForwardWord => Some(ReedlineEvent::Edit(vec![EditCommand::MoveWordRight {
            select: false,
        }])),
        InputFunction::Yank => Some(ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferAfter])),
        InputFunction::ViRedo => Some(ReedlineEvent::Edit(vec![EditCommand::Redo])),
        InputFunction::TransposeChars => {
            Some(ReedlineEvent::Edit(vec![EditCommand::SwapGraphemes]))
        }
        InputFunction::UpcaseWord => Some(ReedlineEvent::Edit(vec![EditCommand::UppercaseWord])),
        InputFunction::Undo => Some(ReedlineEvent::Edit(vec![EditCommand::Undo])),
        InputFunction::ClearScreen => Some(ReedlineEvent::ClearScreen),
        InputFunction::AcceptLine => Some(ReedlineEvent::Enter),
        InputFunction::HistorySearchBackward => Some(ReedlineEvent::SearchHistory),
        InputFunction::RedrawCurrentLine => Some(ReedlineEvent::Repaint),
        InputFunction::Complete => Some(ReedlineEvent::Edit(vec![EditCommand::Complete])),
        InputFunction::BrushAcceptHint => Some(ReedlineEvent::HistoryHintComplete),
        InputFunction::BrushAcceptHintWord => Some(ReedlineEvent::HistoryHintWordComplete),
        // Needs the shell, so it goes back to the host rather than to the editor.
        InputFunction::ShellExpandLine => Some(ReedlineEvent::ExecuteHostCommand(
            HostCommand::InputFunction(func.clone()).encode(),
        )),
        // reedline's Up/Down move a screen line in a multiline buffer and step through
        // history otherwise, so both readline names land on them; history is the one
        // `bind -p` lists, matching bash's default for these keys.
        InputFunction::PreviousHistory | InputFunction::PreviousScreenLine => {
            Some(ReedlineEvent::Up)
        }
        InputFunction::NextHistory | InputFunction::NextScreenLine => Some(ReedlineEvent::Down),
        InputFunction::InsertComment => Some(ReedlineEvent::Multiple(vec![
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToStart { select: false },
                EditCommand::InsertChar('#'),
            ]),
            ReedlineEvent::Enter,
        ])),
        _ => None,
    }
}

#[expect(clippy::too_many_lines)]
pub(super) fn translate_reedline_event_to_action(
    event: &reedline::ReedlineEvent,
) -> Option<KeyAction> {
    match event {
        reedline::ReedlineEvent::Edit(cmds) => {
            match cmds.as_slice() {
                [reedline::EditCommand::Backspace] => Some(KeyAction::DoInputFunction(
                    InputFunction::BackwardDeleteChar,
                )),
                [reedline::EditCommand::BackspaceWord] => {
                    // Not quite accurate, because it doesn't save the deleted text.
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardKillWord))
                }
                [reedline::EditCommand::CapitalizeChar] => None,
                [reedline::EditCommand::ClearToLineEnd] => {
                    // Not quite accurate, because it doesn't save the deleted text.
                    Some(KeyAction::DoInputFunction(InputFunction::KillLine))
                }
                [reedline::EditCommand::Complete] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Complete))
                }
                [reedline::EditCommand::CutFromStart] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWholeLine))
                }
                [reedline::EditCommand::KillLine] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillLine))
                }
                [reedline::EditCommand::CutWordLeft] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardKillWord))
                }
                [reedline::EditCommand::CutWordRight] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWord))
                }
                [reedline::EditCommand::Delete] => {
                    Some(KeyAction::DoInputFunction(InputFunction::DeleteChar))
                }
                [reedline::EditCommand::DeleteWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWord))
                }
                [reedline::EditCommand::InsertNewline] => None,
                [reedline::EditCommand::LowercaseWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::DowncaseWord))
                }
                [reedline::EditCommand::MoveLeft { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardChar))
                }
                [reedline::EditCommand::MoveLeft { select: true }] => None,
                [reedline::EditCommand::MoveRight { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ForwardChar))
                }
                [reedline::EditCommand::MoveRight { select: true }] => None,
                [reedline::EditCommand::MoveToEnd { select: false }] => {
                    // TODO(input): Not quite accurate, because it doesn't just go to end of line.
                    Some(KeyAction::DoInputFunction(InputFunction::EndOfLine))
                }
                [reedline::EditCommand::MoveToEnd { select: true }] => None,
                [reedline::EditCommand::MoveToLineEnd { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::EndOfLine))
                }
                [reedline::EditCommand::MoveToLineEnd { select: true }] => None,
                [reedline::EditCommand::MoveToLineStart { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BeginningOfLine))
                }
                [reedline::EditCommand::MoveToLineStart { select: true }] => None,
                [reedline::EditCommand::MoveToStart { select: false }] => {
                    // TODO(input): Not quite accurate, because it doesn't just go to beginning of
                    // line.
                    Some(KeyAction::DoInputFunction(InputFunction::BeginningOfLine))
                }
                [reedline::EditCommand::MoveToStart { select: true }] => None,
                [reedline::EditCommand::MoveWordLeft { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardWord))
                }
                [reedline::EditCommand::MoveWordLeft { select: true }] => None,
                [reedline::EditCommand::MoveWordRight { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ForwardWord))
                }
                [reedline::EditCommand::MoveWordRight { select: true }] => None,
                [reedline::EditCommand::PasteCutBufferAfter] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Yank))
                }
                [reedline::EditCommand::PasteCutBufferBefore] => None,
                [reedline::EditCommand::Redo] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ViRedo))
                }
                [reedline::EditCommand::SelectAll] => None,
                [reedline::EditCommand::SwapGraphemes] => {
                    Some(KeyAction::DoInputFunction(InputFunction::TransposeChars))
                }
                [reedline::EditCommand::UppercaseWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::UpcaseWord))
                }
                [reedline::EditCommand::Undo] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Undo))
                }
                _ => {
                    // TODO(input): Handle more?
                    tracing::debug!(target: trace_categories::INPUT, "unhandled edit commands: {cmds:?}");
                    None
                }
            }
        }
        reedline::ReedlineEvent::ClearScreen => {
            Some(KeyAction::DoInputFunction(InputFunction::ClearScreen))
        }
        reedline::ReedlineEvent::CtrlC => None,
        reedline::ReedlineEvent::CtrlD => None,
        reedline::ReedlineEvent::Enter => {
            Some(KeyAction::DoInputFunction(InputFunction::AcceptLine))
        }
        reedline::ReedlineEvent::Esc => None,
        reedline::ReedlineEvent::MenuPrevious => None,
        reedline::ReedlineEvent::OpenEditor => None,
        reedline::ReedlineEvent::Left => {
            Some(KeyAction::DoInputFunction(InputFunction::BackwardChar))
        }
        reedline::ReedlineEvent::Right => {
            Some(KeyAction::DoInputFunction(InputFunction::ForwardChar))
        }
        reedline::ReedlineEvent::Up => {
            Some(KeyAction::DoInputFunction(InputFunction::PreviousHistory))
        }
        reedline::ReedlineEvent::Down => {
            Some(KeyAction::DoInputFunction(InputFunction::NextHistory))
        }
        reedline::ReedlineEvent::SearchHistory => Some(KeyAction::DoInputFunction(
            InputFunction::HistorySearchBackward,
        )),
        reedline::ReedlineEvent::Repaint => {
            Some(KeyAction::DoInputFunction(InputFunction::RedrawCurrentLine))
        }
        reedline::ReedlineEvent::HistoryHintComplete => {
            Some(KeyAction::DoInputFunction(InputFunction::BrushAcceptHint))
        }
        reedline::ReedlineEvent::HistoryHintWordComplete => Some(KeyAction::DoInputFunction(
            InputFunction::BrushAcceptHintWord,
        )),
        reedline::ReedlineEvent::Multiple(evts) => {
            if let &[
                reedline::ReedlineEvent::Edit(ref edit_cmds),
                reedline::ReedlineEvent::Enter,
            ] = evts.as_slice()
            {
                if let &[
                    reedline::EditCommand::MoveToStart { select: false },
                    reedline::EditCommand::InsertChar('#'),
                ] = edit_cmds.as_slice()
                {
                    return Some(KeyAction::DoInputFunction(InputFunction::InsertComment));
                }
            }

            // TODO(input): Try to extract something from these?
            tracing::debug!(target: trace_categories::INPUT, "unhandled composite event: {evts:?}");
            None
        }
        reedline::ReedlineEvent::UntilFound(uf_events) => {
            // The menu alternatives only apply while a menu is up; what a key means
            // otherwise is the single event left after them.
            let mut rest = uf_events.iter().skip_while(|event| {
                matches!(
                    event,
                    reedline::ReedlineEvent::HistoryHintComplete
                        | reedline::ReedlineEvent::HistoryHintWordComplete
                        | reedline::ReedlineEvent::Menu(_)
                        | reedline::ReedlineEvent::MenuDown
                        | reedline::ReedlineEvent::MenuUp
                        | reedline::ReedlineEvent::MenuLeft
                        | reedline::ReedlineEvent::MenuRight
                        | reedline::ReedlineEvent::MenuNext
                        | reedline::ReedlineEvent::MenuPrevious
                        | reedline::ReedlineEvent::MenuPageNext
                        | reedline::ReedlineEvent::MenuPagePrevious
                )
            });

            if let (Some(event), None) = (rest.next(), rest.next()) {
                translate_reedline_event_to_action(event)
            } else {
                // TODO(input): Try to extract something from these?
                tracing::debug!(target: trace_categories::INPUT, "unhandled until-found event: {uf_events:?}");
                None
            }
        }
        reedline::ReedlineEvent::ExecuteHostCommand(cmd) => match HostCommand::decode(cmd) {
            HostCommand::Command(cmd) => Some(KeyAction::ShellCommand(cmd)),
            HostCommand::InputFunction(function) => Some(KeyAction::DoInputFunction(function)),
            // Deferred records never live in the bindings; one here is a bug, not a binding.
            HostCommand::Deferred(_) => None,
        },
        evt => {
            // TODO(input): Handle more?
            tracing::debug!(target: trace_categories::INPUT, "unhandled event: {evt:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every readline function the editor can carry out, paired with the event it becomes.
    ///
    /// [`translate_action_to_reedline_event`] and [`translate_reedline_event_to_action`] are
    /// two hand-written tables that have to agree, and nothing but this makes them: the
    /// first decides what a key *does*, the second decides what `bind -p` *calls* it, and an
    /// inputrc built from that listing is fed straight back to `bind`. Enumerating the
    /// functions is what keeps a new arm in one table from being forgotten in the other.
    fn translated_functions() -> impl Iterator<Item = (InputFunction, reedline::ReedlineEvent)> {
        use strum::IntoEnumIterator as _;

        InputFunction::iter().filter_map(|function| {
            let event =
                translate_action_to_reedline_event(&KeyAction::DoInputFunction(function.clone()))?;
            Some((function, event))
        })
    }

    #[test]
    fn every_supported_function_is_one_bind_p_can_name() {
        // A function whose event translates back to nothing would bind fine and then list
        // as a key with no name at all.
        for (function, event) in translated_functions() {
            assert!(
                translate_reedline_event_to_action(&event).is_some(),
                "{function} binds to {event:?}, which `bind -p` cannot name"
            );
        }
    }

    #[test]
    fn a_listed_function_rebinds_to_the_behavior_it_was_listed_from() {
        // The property that makes a `bind -p` listing reloadable: what a key is listed as
        // has to bind back to the same event, even where the name changes on the way (Up is
        // listed as previous-history, which is what next-screen-line's sibling binds to).
        for (function, event) in translated_functions() {
            let Some(listed) = translate_reedline_event_to_action(&event) else {
                continue; // Reported by `every_supported_function_is_one_bind_p_can_name`.
            };
            assert_eq!(
                translate_action_to_reedline_event(&listed),
                Some(event.clone()),
                "{function} binds to {event:?} but lists as `{listed}`, which binds elsewhere"
            );
        }
    }

    #[test]
    fn the_function_table_covers_what_the_event_table_names() {
        // The other direction: every function the second table can produce has to be one
        // the first table binds, or `bind -p` names a key with a function `bind` rejects.
        for (_, event) in translated_functions() {
            if let Some(KeyAction::DoInputFunction(named)) =
                translate_reedline_event_to_action(&event)
            {
                assert!(
                    translate_action_to_reedline_event(&KeyAction::DoInputFunction(named.clone()))
                        .is_some(),
                    "`bind -p` names `{named}`, which `bind` does not accept"
                );
            }
        }
    }

    #[test]
    fn host_command_encoding_round_trips() {
        for host_command in [
            HostCommand::Command("echo hi".to_owned()),
            HostCommand::InputFunction(InputFunction::ShellExpandLine),
            HostCommand::InputFunction(InputFunction::AcceptLine),
            HostCommand::Deferred(0),
            HostCommand::Deferred(7),
            HostCommand::Deferred(usize::MAX),
        ] {
            assert_eq!(HostCommand::decode(&host_command.encode()), host_command);
        }

        // A NUL followed by anything but a record index or the marker is not one.
        for text in [
            "\0nonsense",
            "\0deferred ",
            "\0deferred zz",
            "\0deferred 1 2",
            "\0deferred -1",
            "\0function ",
            "\0function not-a-readline-function",
            "\0shell-expand-line",
        ] {
            assert_eq!(
                HostCommand::decode(text),
                HostCommand::Command(text.to_owned())
            );
        }
        assert_eq!(
            HostCommand::decode("echo hi"),
            HostCommand::Command("echo hi".to_owned())
        );
    }

    #[test]
    fn a_bound_command_survives_the_round_trip_to_an_event_and_back() {
        // Including a NUL anywhere but the front, which is not reserved.
        for command in ["echo hi", "echo a\0b", "printf '%s' \"a b\""] {
            let action = KeyAction::ShellCommand(command.to_owned());
            let event = translate_action_to_reedline_event(&action);
            assert_eq!(
                event.as_ref().and_then(translate_reedline_event_to_action),
                Some(action),
                "{command:?}"
            );
        }
    }

    #[test]
    fn a_command_containing_a_nul_elsewhere_still_round_trips() {
        // Only a *leading* NUL is reserved.
        let command = HostCommand::Command("echo a\0b".to_owned());
        assert_eq!(HostCommand::decode(&command.encode()), command);
    }

    #[test]
    fn flatten_orders_nested_events_and_drops_nothings() {
        use reedline::ReedlineEvent as E;

        assert_eq!(flatten(E::None), vec![]);
        assert_eq!(flatten(E::Enter), vec![E::Enter]);
        assert_eq!(
            flatten(E::Multiple(vec![
                E::None,
                E::Multiple(vec![E::Esc, E::None, E::Multiple(vec![E::Enter])]),
                E::CtrlC,
            ])),
            vec![E::Esc, E::Enter, E::CtrlC]
        );
    }

    #[test]
    fn combine_collapses_to_the_smallest_equivalent_event() {
        use reedline::ReedlineEvent as E;

        assert_eq!(combine(vec![]), E::None);
        assert_eq!(combine(vec![E::None, E::None]), E::None);
        assert_eq!(combine(vec![E::None, E::Enter]), E::Enter);
        assert_eq!(
            combine(vec![E::Esc, E::None, E::Enter]),
            E::Multiple(vec![E::Esc, E::Enter])
        );
    }

    #[test]
    fn a_deferred_record_is_never_mistaken_for_a_binding() {
        // Deferred records live only in the events reedline returns, never in the bindings,
        // so listing must not turn one back into a shell command.
        let encoded = HostCommand::Deferred(3).encode();

        assert_eq!(
            translate_reedline_event_to_action(&reedline::ReedlineEvent::ExecuteHostCommand(
                encoded
            )),
            None
        );
        assert_eq!(
            translate_reedline_event_to_action(&reedline::ReedlineEvent::ExecuteHostCommand(
                "echo hi".to_owned()
            )),
            Some(KeyAction::ShellCommand("echo hi".to_owned()))
        );
    }
}
