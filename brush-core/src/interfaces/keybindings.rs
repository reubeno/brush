use std::{
    collections::BTreeMap,
    fmt::{self, Display, Formatter},
};

/// Represents an action that can be taken in response to a key sequence.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum KeyAction {
    /// Execute a shell command.
    ShellCommand(String),
    /// Execute an input "function".
    DoInputFunction(InputFunction),
}

impl Display for KeyAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShellCommand(command) => write!(f, "shell command: {command}"),
            Self::DoInputFunction(function) => function.fmt(f),
        }
    }
}

/// Defines all input functions. Based on standard `readline` functions,
/// augmented with some `brush`-specific extensions.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    PartialEq,
    strum_macros::EnumString,
    strum_macros::Display,
    strum_macros::EnumIter,
    strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[expect(missing_docs)]
pub enum InputFunction {
    Abort,
    AcceptLine,
    AliasExpandLine,
    ArrowKeyPrefix,
    BackwardByte,
    BackwardChar,
    BackwardDeleteChar,
    BackwardKillLine,
    BackwardKillWord,
    BackwardWord,
    BashViComplete,
    BeginningOfHistory,
    BeginningOfLine,
    BracketedPasteBegin,
    BrushAcceptHint,
    BrushAcceptHintWord,
    CallLastKbdMacro,
    CapitalizeWord,
    CharacterSearch,
    CharacterSearchBackward,
    ClearDisplay,
    ClearScreen,
    Complete,
    CompleteCommand,
    CompleteFilename,
    CompleteHostname,
    CompleteIntoBraces,
    CompleteUsername,
    CompleteVariable,
    CopyBackwardWord,
    CopyForwardWord,
    CopyRegionAsKill,
    DabbrevExpand,
    DeleteChar,
    DeleteCharOrList,
    DeleteHorizontalSpace,
    DigitArgument,
    DisplayShellVersion,
    DoLowercaseVersion,
    DowncaseWord,
    DumpFunctions,
    DumpMacros,
    DumpVariables,
    DynamicCompleteHistory,
    EditAndExecuteCommand,
    EmacsEditingMode,
    EndKbdMacro,
    EndOfHistory,
    EndOfLine,
    ExchangePointAndMark,
    ExecuteNamedCommand,
    ExportCompletions,
    FetchHistory,
    ForwardBackwardDeleteChar,
    ForwardByte,
    ForwardChar,
    ForwardSearchHistory,
    ForwardWord,
    GlobCompleteWord,
    GlobExpandWord,
    GlobListExpansions,
    HistoryAndAliasExpandLine,
    HistoryExpandLine,
    HistorySearchBackward,
    HistorySearchForward,
    HistorySubstringSearchBackward,
    HistorySubstringSearchForward,
    InsertComment,
    InsertCompletions,
    InsertLastArgument,
    KillLine,
    KillRegion,
    KillWholeLine,
    KillWord,
    MagicSpace,
    MenuComplete,
    MenuCompleteBackward,
    NextHistory,
    NextScreenLine,
    NonIncrementalForwardSearchHistory,
    NonIncrementalForwardSearchHistoryAgain,
    NonIncrementalReverseSearchHistory,
    NonIncrementalReverseSearchHistoryAgain,
    OldMenuComplete,
    OperateAndGetNext,
    OverwriteMode,
    PossibleCommandCompletions,
    PossibleCompletions,
    PossibleFilenameCompletions,
    PossibleHostnameCompletions,
    PossibleUsernameCompletions,
    PossibleVariableCompletions,
    PreviousHistory,
    PreviousScreenLine,
    PrintLastKbdMacro,
    QuotedInsert,
    ReReadInitFile,
    RedrawCurrentLine,
    ReverseSearchHistory,
    RevertLine,
    SelfInsert,
    SetMark,
    ShellBackwardKillWord,
    ShellBackwardWord,
    ShellExpandLine,
    ShellForwardWord,
    ShellKillWord,
    ShellTransposeWords,
    SkipCsiSequence,
    SpellCorrectWord,
    StartKbdMacro,
    TabInsert,
    TildeExpand,
    TransposeChars,
    TransposeWords,
    TtyStatus,
    Undo,
    UniversalArgument,
    UnixFilenameRubout,
    UnixLineDiscard,
    UnixWordRubout,
    UpcaseWord,
    ViAppendEol,
    ViAppendMode,
    ViArgDigit,
    #[strum(serialize = "vi-bWord")]
    ViBWord,
    ViBackToIndent,
    ViBackwardBigword,
    ViBackwardWord,
    ViBword,
    ViChangeCase,
    ViChangeChar,
    ViChangeTo,
    ViCharSearch,
    ViColumn,
    ViComplete,
    ViDelete,
    ViDeleteTo,
    #[strum(serialize = "vi-eWord")]
    ViEWord,
    ViEditAndExecuteCommand,
    ViEditingMode,
    ViEndBigword,
    ViEndWord,
    ViEofMaybe,
    ViEword,
    #[strum(serialize = "vi-fWord")]
    ViFWord,
    ViFetchHistory,
    ViFirstPrint,
    ViForwardBigword,
    ViForwardWord,
    ViFword,
    ViGotoMark,
    ViInsertBeg,
    ViInsertionMode,
    ViMatch,
    ViMovementMode,
    ViNextWord,
    ViOverstrike,
    ViOverstrikeDelete,
    ViPrevWord,
    ViPut,
    ViRedo,
    ViReplace,
    ViRubout,
    ViSearch,
    ViSearchAgain,
    ViSetMark,
    ViSubst,
    ViTildeExpand,
    ViUndo,
    ViUnixWordRubout,
    ViYankArg,
    ViYankPop,
    ViYankTo,
    Yank,
    YankLastArg,
    YankNthArg,
    YankPop,
}

/// A key sequence, as the bytes the terminal sends for it: `\C-a` is `0x01`, `\M-f` is
/// `ESC f`, the up arrow is `ESC [ A`. This is also how readline stores key sequences.
///
/// Distinct from [`KeyMacro`] so that a macro body is never passed where a key sequence is
/// expected or the other way round; the two spell high bytes differently.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KeySequence(Vec<u8>);

impl KeySequence {
    /// The bytes of the sequence.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The bytes of the sequence.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for KeySequence {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for KeySequence {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl Display for KeySequence {
    /// Uses octal for high bytes: meta notation would instead name an escape prefix.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            if byte.is_ascii() {
                format_readline_byte(*byte, f)?;
            } else {
                write!(f, r"\{byte:03o}")?;
            }
        }

        Ok(())
    }
}

/// The byte sequence replayed by a key-binding macro.
///
/// Readline macros differ from physical key sequences: their contents are fed back through
/// the active keymap, so they may interleave literal text with control or meta commands.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KeyMacro(Vec<u8>);

impl KeyMacro {
    /// The bytes replayed by this macro.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The bytes replayed by this macro.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for KeyMacro {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for KeyMacro {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl Display for KeyMacro {
    /// Writes macro bytes the way readline's `bind -s` spells them: a control byte is `\C-`
    /// plus the lower-case key it is the control of, DEL is `\C-?`, and a byte with the high
    /// bit set is `\M-` plus the spelling of the byte without it.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            format_readline_byte(*byte, f)?;
        }

        Ok(())
    }
}

fn format_readline_byte(byte: u8, f: &mut Formatter<'_>) -> fmt::Result {
    match byte {
        0x80..=0xff => {
            write!(f, r"\M-")?;
            format_readline_byte(byte & 0x7f, f)
        }
        b'\\' | b'"' => write!(f, "\\{}", byte as char),
        b'\x1b' => write!(f, r"\e"),
        0x7f => write!(f, r"\C-?"),
        0x00..=0x1f => {
            write!(f, r"\C-")?;
            format_readline_byte((byte | 0x40).to_ascii_lowercase(), f)
        }
        _ => write!(f, "{}", byte as char),
    }
}

/// Encapsulates the shell's interaction with key bindings for input.
pub trait KeyBindings: Send {
    /// Retrieves current bindings, in key order.
    fn get_current(&self) -> BTreeMap<KeySequence, KeyAction>;

    /// Sets or updates a binding.
    ///
    /// # Arguments
    ///
    /// * `seq` - The key sequence to bind.
    /// * `action` - The action to bind to the sequence.
    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error>;

    /// Unbinds a key sequence. Returns true if a binding was removed.
    ///
    /// # Arguments
    ///
    /// * `seq` - The key sequence to unbind.
    fn try_unbind(&mut self, seq: KeySequence) -> bool;

    /// Defines a macro that replays bytes through the active keymap.
    ///
    /// # Arguments
    ///
    /// * `seq` - The key sequence to bind the macro to.
    /// * `target` - The bytes replayed by the macro.
    fn define_macro(&mut self, seq: KeySequence, target: KeyMacro) -> Result<(), std::io::Error>;

    /// Retrieves all defined macros, in key order.
    fn get_macros(&self) -> BTreeMap<KeySequence, KeyMacro>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_display_uses_readline_escapes() {
        // Checked against bash: control punctuation is `\C-@`, `\C-\\`, `\C-]`, `\C-^`; high
        // bytes are `\M-` plus the rest; a single quote is not escaped.
        let bytes = b"a\x01\x1b\x1f\x7f\\\"'\x00\x1c\x1d\x1e\x80\xc3\xa9 ~".to_vec();
        assert_eq!(
            KeyMacro::from(bytes).to_string(),
            r#"a\C-a\e\C-_\C-?\\\"'\C-@\C-\\\C-]\C-^\M-\C-@\M-C\M-) ~"#
        );
    }

    #[test]
    fn key_sequence_displays_in_readline_notation() {
        let seq = KeySequence::from(b"\x1b[A\x0d\x7f\x1b\"a\x01".to_vec());
        assert_eq!(seq.to_string(), r#"\e[A\C-m\C-?\e\"a\C-a"#);
    }

    #[test]
    fn key_sequence_high_bytes_use_octal_not_meta() {
        let seq = KeySequence::from(b"\xc3\xa9\x80\xff1".to_vec());
        assert_eq!(seq.to_string(), r"\303\251\200\3771");
    }

    #[test]
    fn key_sequence_display_matches_macro_display() {
        let bytes = b"\x18\x1fA0\x07".to_vec();
        assert_eq!(
            KeySequence::from(bytes.clone()).to_string(),
            KeyMacro::from(bytes).to_string()
        );
    }
}
