use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
};

/// Represents an action that can be taken in response to a key sequence.
pub enum KeyAction {
    /// Execute a shell command.
    ShellCommand(String),
    /// Execute an input "function".
    DoInputFunction(InputFunction),
}

impl Display for KeyAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            KeyAction::ShellCommand(command) => write!(f, "shell command: {command}"),
            KeyAction::DoInputFunction(function) => function.fmt(f),
        }
    }
}

/// Defines all input functions.
#[derive(strum_macros::EnumString, strum_macros::Display, strum_macros::EnumIter)]
#[strum(serialize_all = "kebab-case")]
#[allow(missing_docs)]
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
    BeginningOfHistory,
    BeginningOfLine,
    BracketedPasteBegin,
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
    ViEWord,
    ViEditingMode,
    ViEndBigword,
    ViEndWord,
    ViEofMaybe,
    ViEword,
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
    ViUnixWordRubout,
    ViYankArg,
    ViYankPop,
    ViYankTo,
    Yank,
    YankLastArg,
    YankNthArg,
    YankPop,
}

/// Represents a sequence of keys.
#[derive(Eq, Hash, PartialEq)]
pub struct KeySequence {
    /// The strokes in the sequence.
    pub strokes: Vec<KeyStroke>,
}

impl Display for KeySequence {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for stroke in &self.strokes {
            stroke.fmt(f)?;
        }
        Ok(())
    }
}

impl From<KeyStroke> for KeySequence {
    /// Creates a new key sequence with a single stroke.
    fn from(value: KeyStroke) -> Self {
        Self {
            strokes: vec![value],
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
/// Represents a single key press.
pub struct KeyStroke {
    /// Alt key was pressed.
    pub alt: bool,
    /// Control key was pressed.
    pub control: bool,
    /// Shift key was pressed.
    pub shift: bool,
    /// Primary key pressed.
    pub key: Key,
}

impl Display for KeyStroke {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.alt {
            write!(f, "\\e")?;
        }
        if self.control {
            write!(f, "\\C-")?;
        }
        if self.shift {
            // TODO: Figure out what to do here or if the key encodes the shift in it.
        }
        self.key.fmt(f)
    }
}

impl From<Key> for KeyStroke {
    /// Creates a new key stroke with a single key.
    fn from(value: Key) -> Self {
        Self {
            alt: false,
            control: false,
            shift: false,
            key: value,
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
/// Represents a single key.
pub enum Key {
    /// A simple character key.
    Character(char),
}

impl Display for Key {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Key::Character(c @ ('\\' | '\"' | '\'')) => write!(f, "\\{c}")?,
            Key::Character(c) => write!(f, "{c}")?,
        }

        Ok(())
    }
}

/// Encapsulates the shell's interaction with key bindings for input.
pub trait KeyBindings: Send {
    /// Retrieves current bindings.
    fn get_current(&self) -> HashMap<KeySequence, KeyAction>;
}
