# Key bindings and readline macros

How `bind` turns text into behavior at the prompt, for the default (reedline) input backend.
The other backends do not support `bind`.

## The pieces

| layer | crate | module | what it owns |
|---|---|---|---|
| grammar | `brush-parser` | `readline_binding` | parses `"\C-x": target` into key-sequence items and a target |
| builtin | `brush-builtins` | `bind` | expands parsed items to bytes; lists, binds, unbinds |
| interface | `brush-core` | `interfaces::keybindings` | the key types and the trait the builtin talks to |
| keys | `brush-interactive` | `reedline::keys` | the terminal's key sequences (terminfo plus readline's default table) and the conversions between them and reedline's keys |
| stream | `brush-interactive` | `reedline::pending` | the shared input stream: what bytes can match, where a native event may not be split, canonicalizing the matching window |
| editor | `brush-interactive` | `reedline::edit_mode` | the one implementation of the bindings trait, wrapped around reedline |
| events | `brush-interactive` | `reedline::events` | translation both ways between key actions and reedline events, and the string format bound commands travel in |
| backend | `brush-interactive` | `reedline::input_backend` | runs reedline, hands bound commands to the shell, replays what follows them |

The shell holds the editor's bindings behind an interface; the builtin never sees reedline.

## Public API migration

This design intentionally replaces the previous exported key-binding representation:

- `brush_core::interfaces::Key` and `KeyStroke` are removed. `KeySequence` now stores the
  terminal bytes directly, and `KeyMacro` separately stores replayed macro bytes.
- `KeyAction::Sequence` and `KeyBindings::get_untranslated` are removed. Macro expansion and
  raw-sequence matching are responsibilities of the input backend.
- `KeyBindings::get_current` and `get_macros` return ordered maps, and `define_macro` accepts
  a `KeyMacro`.
- `brush_parser::readline_binding::KeyStroke` and `key_sequence_to_strokes` are replaced by
  `key_sequence_to_bytes` and `macro_sequence_to_bytes`.
- `brush_core::sys::input` is removed. Terminal-specific key decoding now lives beside its
  only consumer in `brush-interactive`.

These are breaking changes for embedders using the old interfaces. Compatibility shims are
not provided because the old stroke-based types cannot faithfully represent arbitrary byte
sequences or distinguish binding triggers from macro bodies.

## Vocabulary

- **Key sequence**: the bytes the terminal sends for a key sequence, as readline stores
  them. `\C-g` is `0x07`, `\M-f` is `ESC f`, the up arrow is `ESC [ A`, and a multi-key
  chain such as atuin's `\C-x\C-_A0\C-g` is a longer sequence.
- **Action**: what a non-macro key does: a shell command (`bind -x`) or a readline function
  (`bind '"\C-l": clear-screen'`).
- **Macro**: the bytes a macro replays (`bind '"\C-g": "echo hi\C-m"'`). A distinct type
  from a key sequence because the two spell high bytes differently.

A key holds either an action or a macro, never both; defining one removes the other, as in
bash.

### Notation

Trigger and macro-body notation differ for Meta:

- In a trigger, `\M-f` names `ESC f`. A high byte in a trigger lists as a three-digit octal
  escape (`\303`).
- In a macro body, `\M-f` sets the high bit (`0xe6`); use `\ef` for an escape prefix.
  `bind -s` lists a high byte as `\M-` plus the rest, so `\M-C\M-)` is the UTF-8 bytes
  `c3 a9`.

Either listing reloads as the bytes it came from. readline itself stores `\M-f` as the
high-bit byte on both sides and folds an incoming `ESC f` into it at dispatch time
(`convert-meta`); brush keeps the escape prefix on the trigger side and matches the bytes the
terminal actually sends.

Literal text is UTF-8. Numeric escapes (`\nnn`, `\xHH`) preserve individual bytes, NUL
included; an octal value above a byte keeps its low byte. An unrecognized escape loses its
backslash and keeps its character (`\z` is `z`); `\C`, `\M` and `\x` without what must
follow them are unrecognized too.

## Keys and bytes

reedline identifies a key as a modifier set plus a key code. The keys module converts each
way between that and bytes, mirroring what crossterm reports in raw mode:

- `0x01`–`0x1a` are Ctrl plus a letter; `0x1c`–`0x1f` are Ctrl plus `4`–`7`; `0x00` is
  Ctrl+Space.
- `\r` is Enter, `\t` is Tab, DEL is Backspace. `\n` is Ctrl+J, bound to accept-line by
  default as in readline.
- `ESC` plus a key is that key with Alt, whether the two arrive as one event or as two
  keystrokes. `ESC ESC` is the escape key followed by whatever comes next, so `\e\ef` is
  never one key.
- Cursor and function key sequences are looked up in the platform's terminal database
  (terminfo on unix) first, then in readline's default table, which covers both the CSI
  (`\e[A`) and SS3 (`\eOA`) forms. Single bytes skip both tables.
- Modified forms fold their parameter in: `\e[1;5C` is Ctrl+Right, `\e[3;5~` is Ctrl+Delete.
  `\e[Z` is Shift+BackTab.
- Shift is ignored on printable characters without Control, so a binding on `A` or `\M-F`
  fires even though the terminal reports Shift. A key whose other modifiers cannot survive
  a byte round trip (Shift+Enter, Ctrl+Backspace) keeps its native editor behavior and
  never matches a binding.

A sequence that lifts to exactly one key is a **key**; anything else is **raw**. A key is at
most eight bytes; a longer terminfo entry is raw.

The **canonical** form of a sequence respells it key by key, so `\eOA` and `\e[A` name the
same key and `\C-x\eOA` the same two-key sequence as `\C-x\e[A`. Bindings and macro keys are
canonicalized as they are stored: `bind -r` finds a binding by either spelling and listings
print the canonical one (`\e[1;5~` lists as `\e[1;5H`). A macro body is stored as typed, so
`bind -s` lists it faithfully, and canonicalized as it is resolved.

Invariant, checked by a property test over every byte, every default sequence with every
modifier, and the meta forms: lifting a key's spelling gives back that key, and a sequence's
canonical form is its key's spelling.

## What the editor holds

The editor keeps two stores, both keyed by canonical sequences:

- **base bindings**: reedline's own key map, for single keys bound to actions. reedline
  dispatches from it; the Emacs edit mode is rebuilt from a clone whenever it changes.
- **byte bindings**: one trie holding macros (on a key or a longer sequence) and actions on
  sequences that are not a single key. Resolution needs longest-prefix and prefix-of lookups
  over it.

One pending-input deque holds native terminal events and macro-byte chunks, in order. Native
events keep their exact modifiers. A per-read store holds the records deferred behind
host-return events (below).

Listing (`bind -p`, `-P`, `-X`, `-q`, `-u`) walks the base map and translates each reedline
event back to a readline function name; an event with no readline equivalent is omitted. The
trie is listed as stored. Where two reedline keys spell the same sequence (the terminal
reports `A` as Shift+`A`), the listing takes the one with the fewest modifiers, then the
lowest modifier bits. All listings are in key (byte) order.

## A key is pressed

reedline hands each raw terminal event to the editor, which appends it to the pending
stream, with a byte spelling when that preserves its meaning. Resize events bypass the
stream. Then, until the stream is empty or needs more input:

1. If some bound sequence is longer than the available bytes and starts with them, wait:
   nothing fires, and there is no key-sequence timeout. A pressed lone Esc is held the same
   way, as readline's meta prefix (below).
2. Otherwise the longest bound prefix fires as the macro or action it holds. Matching never
   splits a native terminal event. A macro's body is prepended to the same stream.
3. Failing that, a native key takes its ordinary meaning, or macro bytes are decoded as a
   key (below). Both go through the same lookup, so a pressed key and a replayed one resolve
   identically, readline's meta upper-case fallback (`do-lowercase-version`) included.
4. The remaining input is dispatched again from step 1, so a sequence or macro key can start
   anywhere in it: with `\C-a\C-e` bound, `\C-a` then `\C-t` runs beginning-of-line and then
   whatever `\C-t` is.

**Esc as meta prefix.** A pressed Esc is held as the first byte of a meta key, so `\e` then
`f` is Alt+f exactly as if the terminal had sent them together. Because there is no timeout,
Esc's own meaning (dismissing a completion menu, cancelling a history search) fires as soon
as it is pressed; the miss that may follow does not repeat it. If the pair is bound to
nothing, the escape keeps its own meaning and the key after it is dispatched afresh. A run of
pressed escapes pairs off, so whether the key after the run is a meta key depends on the
run's parity, unless something else (a bound `\ec`, say) independently holds the last escape
as a prefix. Only a pressed escape waits; one replayed from a macro body already has whatever
follows it in the stream.

**Barriers.** A key with no byte spelling, and every non-key event other than resize
(bracketed paste today), is a matching barrier: a held prefix takes its ordinary meaning
before the event is applied. If earlier input returns to the host, the rest of the stream,
barrier included, is deferred rather than discarded. Input still held at an ordinary read
boundary is dropped.

Macros are stored only as bytes and resolved against the bindings as they are when the key is
pressed, so a macro that refers to another sees that macro's current definition.

## Resolving macro bytes

Every step consumes a nonempty byte prefix or one barrier. Empty triggers are ignored; an
empty macro body still consumes its nonempty trigger, which is how atuin's placeholder chains
are consumed.

1. **Longest bound prefix**, whether it holds an action or a macro; a shorter one is tried
   when the longest would split a native event.
2. **Longest key** (up to eight bytes) otherwise, looked up first as a macro trigger, then in
   the base bindings, with the meta upper-case fallback. A bound key produces its event. An
   unbound plain character becomes literal text, and adjacent text coalesces into one
   insert; an unbound escape is the escape key; any other unbound key is dropped, as is a
   byte that starts no key.

When a step lands on a macro, its bytes are prepended rather than resolved in an isolated
recursive call, so a trigger, terminal escape sequence or UTF-8 character can span nested
chunks, and a macro ending in a prefix can wait for keyboard input. Canonicalization covers
only the matching window (the longest bound sequence, at least one key's worth), so splicing
a child macro does not re-normalize the rest of the outer body.

Each keystroke carries a budget of macro bytes it may replay (16 KiB; atuin, fzf and zoxide
replay under a hundred each), and every expansion charges its body's length to it, so a
wide body spends it as surely as a deep chain does; input deferred behind a bound command
carries what is left of it. Running out abandons the whole keystroke, resolved events
included, leaving the buffer as it was. The budget is in bytes rather than nesting depth
because resolution costs time per byte replayed, and a depth limit alone would leave a
runaway binding free to stall the shell for as long as its body is wide.

Resolution stops at the first **bound shell command**, and at the first **accept-line with
input after it**, at any nesting depth (`\M-#` ends in one), because reedline returns to the
host on either and forgets what follows. The events before the stop are kept; the action and
the remaining stream go into a deferred record, unless the stop is a bound command with
nothing behind it, which is returned as itself. The input after a `bind -x` command is
resolved only once the command returns, so the command may rebind it first, which is how
atuin's accept path works.

## Carrying input across reedline's read loop

reedline's read loop returns a string for a bound command and accepts no events from
outside. That string is either a plain command, a marker naming a deferred record the editor
holds, or a marker naming a readline function the editor cannot carry out itself
(`shell-expand-line`). The markers begin with a NUL, which no `bind -x` command can start
with.

When a read completes, the backend ends it on the editor with the record reedline returned,
if any: that claims the record and drops the pending input and every unclaimed record in one
step, so a record ignored during history search cannot be mistaken for a later command's
continuation. A bound command is returned to the shell with its stream pending for the next
read. An accept-line is carried out by an immediately accepting read (a reedline option that
skips the validator, as readline's accept-line does), with its stream pending for the read
after. A marker naming no held record is traced and the shell prompts again.

On the next read the backend resumes the stream against the bindings as they are now, then:

- edit events are applied to the buffer directly;
- an accept-line accepts the buffer immediately;
- another bound command is returned, with what follows recorded again;
- anything that needs reedline's read loop (menus, history search, completion, cursor keys)
  is dropped with a trace.

## `shell-expand-line`

readline's `\M-\C-e` expands the line in place; the fzf and zoxide widgets end their macros
with it. It is bound as a host command naming the function; the backend carries it out itself
and resumes the same read, so on success or failure no prompt hook or prompt substitution
runs again. The expansion, in brush-core, applies parameter, command and arithmetic expansion
with quote removal, expands a tilde prefix at the very start of the buffer (`~/x` but not
`echo ~` or `a=~`, which is where bash's whole-line expansion finds one), preserves literal
source text (whitespace and newlines included), joins backslash-newline continuations
outside single quotes, replaces the whole edit buffer and leaves the cursor at the end. Only
unquoted expansion results undergo IFS splitting, on the same splitter command execution
uses, with fields joined by spaces. Inside a macro it stops resolution like any bound
command, so what follows replays against the expanded line. bash also applies alias and
history expansion here; brush does not yet.

## Known divergences from bash

Because reedline's read loop is closed:

- Events that need it cannot be replayed after a bound command (above). Tracked under #380.
- A macro pressed while reedline is in history-search mode is ignored.
- A macro that enters history search can execute its query as a command instead of selecting
  the match, because reedline dispatches a compound event through its ordinary handler after
  the mode changes. Tracked under #380 pending an upstream reedline change.
- The replay budget stops a macro where readline would stop at sixteen nested levels; a
  macro that reaches itself through a bound command runs that command until the budget is
  spent before giving up. readline inserts what the sixteen levels produced; brush abandons
  the keystroke.
- A byte in a macro body that is not a key and not part of a UTF-8 character (`\M-f`
  stored as `0xe6`, or a bare `\351`) is dropped; readline inserts it as a raw byte.
- There is no key-sequence timeout: a key that is a proper prefix of a longer binding does
  nothing until the next key arrives. A lone Esc with its default meaning fires at once.
- `ESC ESC` then a key: readline reads the two escapes as one sequence (bash binds it to
  `complete`). brush pairs them off the same way but has nothing bound to `\e\e`, so each
  escape only takes its own meaning; a bound prefix such as fzf's `\ec` holding the second
  escape makes the key after it a meta key.
- After a miss, an unbound prefix key takes its ordinary meaning and the rest is
  re-dispatched; readline aborts the whole sequence with a bell.
- An accept-line inside a macro skips the validator, so an incomplete line is submitted as
  it stands.

IFS field splitting:

- `shell-expand-line` splits on the shared execution splitter, which does not yet treat an
  unquoted array or positional expansion the way bash does: as its elements joined on the
  first IFS character and then field-split. So under `IFS=' :'` bash drops an empty element
  and folds a `:` beside an element boundary into it (`A=(aa '' bb)` and `A=(aa ':bb')`
  both give `aa bb` in bash and `aa  bb` here), and under `IFS=:` a `:` ending an element
  leaves an empty field (`A=('aa:' bb)` gives `aa  bb` in bash and `aa bb` here). Scalar
  expansions split as bash does, which is all the fzf and zoxide widgets need. The
  known-failure cases in `compat/ifs.yaml` pin the same gaps for command execution;
  `shell-expand-line` deliberately shares the splitter so that one fix corrects both. The
  cases are tabled in the pty tests for `shell-expand-line`, checked against real bash
  there, and brush's current output is asserted alongside so the fix cannot land without
  updating them.

Listing:

- `bind -s` prints a macro body containing NUL in full; bash stops at the NUL.
- A trigger's `\M-x` lists as `\ex` and a high byte in a trigger as octal, where bash prints
  `\M-` forms. Both reload as what they came from; only the text differs.

Because the shell, not readline, runs bound commands, `READLINE_MARK` and similar are not
set.

Not implemented: `bind -v` and `bind -V` report a fixed emacs keymap (no vi mode, #984);
`bind -f` (reading an inputrc file) returns an error.

## Where the tests are

- grammar and byte expansion: `brush-parser/src/readline_binding.rs`
- the builtin's parsing: `brush-builtins/src/bind.rs`
- key display: `brush-core/src/interfaces/keybindings.rs`
- action/event translation and the bound-command string format:
  `brush-interactive/src/reedline/events.rs`
- key tables, lifting, spelling and canonical form:
  `brush-interactive/src/reedline/keys.rs` (needs `--features reedline`)
- the input stream's own rules, including bounded normalization work:
  `brush-interactive/src/reedline/pending.rs` (needs `--features reedline`)
- sequence matching and re-dispatch, the meta prefix, resolution, recursion, deferral
  and paste ordering: `brush-interactive/src/reedline/edit_mode.rs`
  (needs `--features reedline`)
- replay planning and settling: `brush-interactive/src/reedline/input_backend.rs`
- line expansion and complete buffer replacement: `brush-core/src/expansion.rs`,
  `brush-interactive/src/reedline/input_backend.rs`, and
  `brush-shell/tests/pty_shell_expand_line_tests.rs`
- real terminal and real bash: `brush-shell/tests/pty_readline_macro_tests.rs` and
  `brush-shell/tests/cases/compat/builtins/bind.yaml`; PTY coverage includes bracketed paste
  across held prefixes and host commands. The dropped-event divergence is pinned there by an
  active test, with the bash behavior as an ignored test next to it
- real applications: `cargo xtask test e2e atuin` and `cargo xtask test e2e fzf`
