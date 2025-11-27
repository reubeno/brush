//! Call stack representations.

use crate::{functions, traps};
use std::{borrow::Cow, collections::VecDeque, fmt::Display, sync::Arc};

use brush_parser::ast::SourceLocation;

/// Encapsulates info regarding a script call.
#[derive(Clone, Debug)]
pub struct ScriptCall {
    /// The type of script call.
    pub call_type: ScriptCallType,
    /// The source info for the script called.
    pub source_info: crate::SourceInfo,
}

impl ScriptCall {
    /// Returns the name of the script call target.
    pub fn name(&self) -> Cow<'_, str> {
        self.source_info.source.as_str().into()
    }
}

/// The type of script call.
#[derive(Clone, Debug)]
pub enum ScriptCallType {
    /// A script was sourced.
    Source,
    /// A script was executed.
    Run,
}

impl std::fmt::Display for ScriptCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.call_type {
            ScriptCallType::Source => write!(f, "source({})", self.source_info),
            ScriptCallType::Run => write!(f, "script({})", self.source_info),
        }
    }
}

/// Represents the target of a control flow "call" that invoked a
/// a different source context.
#[derive(Clone, Debug)]
pub enum CallTarget {
    /// A script was called (sourced or executed).
    Script(ScriptCall),
    /// A function was called.
    Function(FunctionCall),
    /// A trap handler was invoked.
    TrapHandler,
    /// A string was eval'd.
    Eval,
    /// A command-line string (i.e., -c) was executed.
    CommandString,
    /// An interactive command session was started.
    InteractiveSession,
}

impl CallTarget {
    /// Returns the name of the call target (i.e., script path or function name).
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Self::Script(call) => call.name(),
            Self::Function(call) => call.name(),
            Self::TrapHandler => "trap".into(),
            Self::Eval => "eval".into(),
            Self::CommandString => "-c".into(),
            Self::InteractiveSession => "interactive".into(),
        }
    }

    /// Returns `true` if the call target is a function call.
    pub const fn is_function(&self) -> bool {
        matches!(self, Self::Function(..))
    }

    /// Returns `true` if the call target is a script call.
    pub const fn is_script(&self) -> bool {
        matches!(self, Self::Script(..))
    }

    /// Returns `true` if the call target is a trap handler.
    pub const fn is_trap_handler(&self) -> bool {
        matches!(self, Self::TrapHandler)
    }

    /// Returns `true` if the call target is an interactive session.
    pub const fn is_interactive_session(&self) -> bool {
        matches!(self, Self::InteractiveSession)
    }

    /// Returns `true` if the call target is a sourced script.
    pub const fn is_sourced_script(&self) -> bool {
        matches!(self, Self::Script(call) if matches!(call.call_type, ScriptCallType::Source))
    }

    /// Returns `true` if the call target is a run script.
    pub const fn is_run_script(&self) -> bool {
        matches!(self, Self::Script(call) if matches!(call.call_type, ScriptCallType::Run))
    }
}

impl std::fmt::Display for CallTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Script(call) => call.fmt(f),
            Self::Function(call) => call.fmt(f),
            Self::TrapHandler => write!(f, "trap"),
            Self::Eval => write!(f, "eval"),
            Self::CommandString => write!(f, "-c"),
            Self::InteractiveSession => write!(f, "interactive"),
        }
    }
}

/// Represents the target of a function call.
#[derive(Clone, Debug)]
pub struct FunctionCall {
    /// The name of the function invoked.
    pub function_name: String,
    /// The invoked function.
    pub function: functions::Registration,
}

impl FunctionCall {
    /// Returns the name of the script call target.
    pub fn name(&self) -> Cow<'_, str> {
        self.function_name.as_str().into()
    }
}

impl std::fmt::Display for FunctionCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "func({}", self.function_name)?;

        if let Some(loc) = &self.function.definition().location() {
            write!(f, " @ {}", loc.start)?;
        }

        write!(f, ")")?;

        Ok(())
    }
}

/// Information about a call site.
#[derive(Clone, Debug, Default)]
pub struct SourceSite {
    /// Info regarding the containing source text.
    pub source_info: crate::SourceInfo,
    /// The relative location of the call site within the source text, if available.
    pub position: Option<Arc<crate::SourcePosition>>,
}

impl Display for SourceSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source_info.source)?;
        if let Some(pos) = &self.position {
            write!(f, ":{},{}", pos.line, pos.column)?;
        }

        Ok(())
    }
}

/// Represents a single frame in a `CallStack`.
#[derive(Clone, Debug)]
pub struct CallFrame {
    /// The type of call that resulted in this frame.
    pub call_target: CallTarget,
    /// Information about the currently executing location. For the topmost frame on
    /// the stack, this represents the current execution location. For older frames,
    /// this represents the site from which a control transfer was made to the next
    /// younger frame.
    pub current: SourceSite,
    /// Positional arguments (not including the name of the target). May not be
    /// present for all frames.
    pub args: Vec<String>,
}

/// Encapsulates a script call stack.
#[derive(Clone, Debug, Default)]
pub struct CallStack {
    frames: VecDeque<CallFrame>,
    func_call_depth: usize,
    script_call_depth: usize,
    trap_handler_depth: usize,
}

impl std::fmt::Display for CallStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            return Ok(());
        }

        writeln!(f, "Call stack (most recent first):")?;

        for (index, frame) in self.iter().enumerate() {
            writeln!(
                f,
                "  #{}| {} (current: {})",
                index, frame.call_target, frame.current,
            )?;
        }

        Ok(())
    }
}

impl CallStack {
    /// Creates a new empty script call stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes the top from from the stack. If the stack is empty, does nothing and
    /// returns `None`; otherwise, returns the removed call frame.
    pub fn pop(&mut self) -> Option<CallFrame> {
        let frame = self.frames.pop_front()?;

        if frame.call_target.is_function() {
            self.func_call_depth = self.func_call_depth.saturating_sub(1);
        }

        if frame.call_target.is_script() {
            self.script_call_depth = self.script_call_depth.saturating_sub(1);
        }

        if frame.call_target.is_trap_handler() {
            self.trap_handler_depth = self.trap_handler_depth.saturating_sub(1);
        }

        Some(frame)
    }

    /// Returns a reference to the current (topmost) call frame in the stack.
    /// Returns `None` if the stack is empty.
    pub fn current_frame(&self) -> Option<&CallFrame> {
        self.frames.front()
    }

    /// Returns the source info for the current (topmost) call frame in the stack.
    /// If the stack is empty, returns a default `SourceInfo`.
    pub fn current_source_info(&self) -> crate::SourceInfo {
        if let Some(frame) = self.frames.front() {
            frame.current.source_info.clone()
        } else {
            crate::SourceInfo::default()
        }
    }

    /// Updates the currently executing position in the top stack frame.
    pub fn set_current_pos(&mut self, position: Option<Arc<crate::SourcePosition>>) {
        if let Some(frame) = self.frames.front_mut() {
            frame.current.position = position;
        }
    }

    /// Pushes a new script call frame onto the stack.
    ///
    /// # Arguments
    ///
    /// * `call_type` - The type of script call (sourced or executed).
    /// * `source_info` - The source of the script.
    /// * `args` - The positional arguments for the script call.
    /// * `call_site` - Information about the call site.
    pub fn push_script(
        &mut self,
        call_type: ScriptCallType,
        source_info: &crate::SourceInfo,
        args: impl IntoIterator<Item = String>,
    ) {
        self.frames.push_front(CallFrame {
            call_target: CallTarget::Script(ScriptCall {
                call_type,
                source_info: source_info.to_owned(),
            }),
            args: args.into_iter().collect(),
            current: SourceSite {
                source_info: source_info.to_owned(),
                position: None,
            },
        });

        self.script_call_depth += 1;
    }

    /// Pushes a new trap handler frame onto the stack.
    pub fn push_trap_handler(&mut self, handler: Option<&traps::TrapHandler>) {
        let source_info =
            handler.map_or_else(crate::SourceInfo::default, |h| h.source_info.clone());

        self.frames.push_front(CallFrame {
            call_target: CallTarget::TrapHandler,
            args: vec![],
            current: SourceSite {
                source_info,
                position: None, // TODO(source-info): Fill in position
            },
        });

        self.trap_handler_depth += 1;
    }

    /// Pushes a new eval frame onto the stack.
    pub fn push_eval(&mut self) {
        self.frames.push_front(CallFrame {
            call_target: CallTarget::Eval,
            args: vec![],
            current: SourceSite::default(), // TODO(source-info): Fill in real source info
        });
    }

    /// Pushes a new command string frame onto the stack.
    pub fn push_command_string(&mut self) {
        self.frames.push_front(CallFrame {
            call_target: CallTarget::CommandString,
            args: vec![],
            current: SourceSite::default(), // TODO(source-info): Fill in real source info
        });
    }

    /// Pushes a new interactive session frame onto the stack.
    pub fn push_interactive_session(&mut self) {
        self.frames.push_front(CallFrame {
            call_target: CallTarget::InteractiveSession,
            args: vec![],
            current: SourceSite::default(), // TODO(source-info): Fill in real source info
        });
    }

    /// Pushes a new function call frame onto the stack.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being called.
    /// * `function` - The function being called.
    /// * `args` - The positional arguments for the function call.
    /// * `call_site` - Information about the call site.
    pub fn push_function(
        &mut self,
        name: impl Into<String>,
        function: &functions::Registration,
        args: impl IntoIterator<Item = String>,
    ) {
        self.frames.push_front(CallFrame {
            call_target: CallTarget::Function(FunctionCall {
                function_name: name.into(),
                function: function.to_owned(),
            }),
            args: args.into_iter().collect(),
            current: SourceSite {
                source_info: function.source().clone(),
                position: function.definition().location().map(|span| span.start),
            },
        });

        self.func_call_depth += 1;
    }

    /// Iterates through the function calls on the stack.
    pub fn iter_function_calls(&self) -> impl Iterator<Item = &FunctionCall> {
        self.iter().filter_map(|frame| {
            if let CallTarget::Function(call) = &frame.call_target {
                Some(call)
            } else {
                None
            }
        })
    }

    /// Iterates through the script calls on the stack.
    pub fn iter_script_calls(&self) -> impl Iterator<Item = &ScriptCall> {
        self.iter().filter_map(|frame| {
            if let CallTarget::Script(call) = &frame.call_target {
                Some(call)
            } else {
                None
            }
        })
    }

    /// Returns whether or not the current script stack frame is a sourced script.
    pub fn in_sourced_script(&self) -> bool {
        self.iter_script_calls()
            .next()
            .is_some_and(|call| matches!(call.call_type, ScriptCallType::Source))
    }

    /// Returns the current depth of function calls in the call stack.
    pub const fn function_call_depth(&self) -> usize {
        self.func_call_depth
    }

    /// Returns the current depth of script calls in the call stack.
    pub const fn script_call_depth(&self) -> usize {
        self.script_call_depth
    }

    /// Returns the current depth of trap handlers in the call stack.
    pub const fn trap_handler_depth(&self) -> usize {
        self.trap_handler_depth
    }

    /// Returns whether or not the shell is actively executing in a shell function.
    pub fn in_function(&self) -> bool {
        self.iter_function_calls().next().is_some()
    }

    /// Returns the current depth of the call stack.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Returns whether or not the call stack is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Returns an iterator over the call frames, starting from the most
    /// recent.
    pub fn iter(&self) -> impl Iterator<Item = &CallFrame> {
        self.frames.iter()
    }

    /// Returns a mutable iterator over the call frames, starting from the most
    /// recent.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut CallFrame> {
        self.frames.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::SourceInfo;
    use pretty_assertions::assert_matches;

    #[test]
    fn test_call_stack_new() {
        let stack = CallStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_call_stack_default() {
        let stack = CallStack::default();
        assert!(stack.is_empty());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_call_stack_push_pop() {
        let mut stack = CallStack::new();

        stack.push_script(
            ScriptCallType::Source,
            &SourceInfo::from(PathBuf::from("script1.sh")),
            vec![],
        );
        assert!(!stack.is_empty());
        assert_eq!(stack.depth(), 1);

        stack.push_script(
            ScriptCallType::Run,
            &SourceInfo::from(PathBuf::from("script2.sh")),
            vec![],
        );
        assert_eq!(stack.depth(), 2);

        let frame = stack.pop().unwrap();
        assert_matches!(
            frame.call_target,
            CallTarget::Script(ScriptCall {
                call_type: ScriptCallType::Run,
                source_info: SourceInfo {
                    source: file_path,
                    ..
                },
            }) if &file_path == "script2.sh"
        );
        assert_eq!(stack.depth(), 1);

        let frame = stack.pop().unwrap();
        assert_matches!(
            frame.call_target,
            CallTarget::Script(ScriptCall {
                call_type: ScriptCallType::Source,
                source_info: SourceInfo {
                    source: file_path,
                    ..
                },
            }) if &file_path == "script1.sh"
        );
        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_call_stack_pop_empty() {
        let mut stack = CallStack::new();
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_in_sourced_script() {
        let mut stack = CallStack::new();
        assert!(!stack.in_sourced_script());

        stack.push_script(
            ScriptCallType::Run,
            &SourceInfo::from(PathBuf::from("script1.sh")),
            vec![],
        );
        assert!(!stack.in_sourced_script());

        stack.push_script(
            ScriptCallType::Source,
            &SourceInfo::from(PathBuf::from("script2.sh")),
            vec![],
        );
        assert!(stack.in_sourced_script());

        stack.pop();
        assert!(!stack.in_sourced_script());
    }

    #[test]
    fn test_call_stack_iter() {
        let mut stack = CallStack::new();
        stack.push_script(
            ScriptCallType::Source,
            &SourceInfo::from(PathBuf::from("script1.sh")),
            vec![],
        );
        stack.push_script(
            ScriptCallType::Run,
            &SourceInfo::from(PathBuf::from("script2.sh")),
            vec![],
        );
        stack.push_script(
            ScriptCallType::Source,
            &SourceInfo::from(PathBuf::from("script3.sh")),
            vec![],
        );

        let frames: Vec<_> = stack.iter().collect();
        assert_eq!(frames.len(), 3);
        assert_matches!(&frames[0].call_target, CallTarget::Script(ScriptCall { source_info: SourceInfo { source: file_path, .. }, .. }) if file_path == "script3.sh");
        assert_matches!(&frames[1].call_target, CallTarget::Script(ScriptCall { source_info: SourceInfo { source: file_path, .. }, .. }) if file_path == "script2.sh");
        assert_matches!(&frames[2].call_target, CallTarget::Script(ScriptCall { source_info: SourceInfo { source: file_path, .. }, .. }) if file_path == "script1.sh");
    }

    #[test]
    fn test_call_stack_display_empty() {
        let stack = CallStack::new();
        assert_eq!(stack.to_string(), "");
    }

    #[test]
    fn test_call_stack_display_with_frames() {
        let mut stack = CallStack::new();
        stack.push_script(
            ScriptCallType::Source,
            &SourceInfo::from(PathBuf::from("script1.sh")),
            vec![],
        );
        stack.push_script(
            ScriptCallType::Run,
            &SourceInfo::from(PathBuf::from("script2.sh")),
            vec![],
        );

        let output = stack.to_string();
        assert!(output.contains("Call stack (most recent first):"));
        assert!(output.contains("#0| run(script2.sh)"));
        assert!(output.contains("#1| source(script1.sh)"));
    }
}
