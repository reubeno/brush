use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
};

/// Represents an action that can be taken in response to a key sequence.
#[derive(Debug)]
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

/// Defines all input functions.
#[derive(Debug, strum_macros::EnumString, strum_macros::Display, strum_macros::EnumIter)]
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
#[derive(Debug, Eq, Hash, PartialEq)]
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

#[derive(Debug, Eq, Hash, PartialEq)]
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// Represents a single key.
pub enum Key {
    /// A simple character key.
    Character(char),
    /// Backspace key.
    Backspace,
    /// Enter key.
    Enter,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page up key.
    PageUp,
    /// Page down key.
    PageDown,
    /// Tab key.
    Tab,
    /// Shift + Tab key.
    BackTab,
    /// Delete key.
    Delete,
    /// Insert key.
    Insert,
    /// F key.
    F(u8),
    /// Escape key.
    Escape,
}

impl Display for Key {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(c @ ('\\' | '\"' | '\'')) => write!(f, "\\{c}")?,
            Self::Character(c) => write!(f, "{c}")?,
            Self::Backspace => write!(f, "Backspace")?,
            Self::Enter => write!(f, "Enter")?,
            Self::Left => write!(f, "Left")?,
            Self::Right => write!(f, "Right")?,
            Self::Up => write!(f, "Up")?,
            Self::Down => write!(f, "Down")?,
            Self::Home => write!(f, "Home")?,
            Self::End => write!(f, "End")?,
            Self::PageUp => write!(f, "PageUp")?,
            Self::PageDown => write!(f, "PageDown")?,
            Self::Tab => write!(f, "Tab")?,
            Self::BackTab => write!(f, "BackTab")?,
            Self::Delete => write!(f, "Delete")?,
            Self::Insert => write!(f, "Insert")?,
            Self::F(n) => write!(f, "F{n}")?,
            Self::Escape => write!(f, "Esc")?,
        }

        Ok(())
    }
}

/// Encapsulates the shell's interaction with key bindings for input.
pub trait KeyBindings: Send {
    /// Retrieves current bindings.
    fn get_current(&self) -> HashMap<KeySequence, KeyAction>;
    /// Updates a binding.
    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error>;
}
