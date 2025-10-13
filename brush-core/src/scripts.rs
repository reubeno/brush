//! Call stack representations.

use std::collections::VecDeque;

/// Represents an executing script.
#[derive(Clone, Debug)]
pub enum CallType {
    /// The script was sourced.
    Sourced,
    /// The script was executed.
    Executed,
}

impl std::fmt::Display for CallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sourced => write!(f, "sourced"),
            Self::Executed => write!(f, "executed"),
        }
    }
}

/// Represents a single frame in a script call stack.
#[derive(Clone, Debug)]
pub struct CallFrame {
    /// The type of script call that resulted in this frame.
    pub call_type: CallType,
    /// The source of the script (e.g., file path).
    pub source: String,
}

/// Encapsulates a script call stack.
#[derive(Clone, Debug, Default)]
pub struct CallStack {
    frames: VecDeque<CallFrame>,
}

impl std::fmt::Display for CallStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            return Ok(());
        }

        writeln!(f, "Script call stack (most recent first):")?;

        for (index, frame) in self.iter().enumerate() {
            writeln!(f, "  #{}| {} ({})", index, frame.source, frame.call_type)?;
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
        self.frames.pop_front()
    }

    /// Pushes a new frame onto the stack.
    ///
    /// # Arguments
    ///
    /// * `call_type` - The type of script call (sourced or executed).
    /// * `source` - The source of the script (e.g., file path).
    pub fn push(&mut self, call_type: CallType, source: impl Into<String>) {
        self.frames.push_front(CallFrame {
            call_type,
            source: source.into(),
        });
    }

    /// Returns whether or not the current script stack frame is a sourced script.
    pub fn in_sourced_script(&self) -> bool {
        self.frames
            .front()
            .is_some_and(|frame| matches!(frame.call_type, CallType::Sourced))
    }

    /// Returns the current depth of the script call stack.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Returns whether or not the script call stack is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Returns an iterator over the script call frames, starting from the most
    /// recent.
    pub fn iter(&self) -> impl Iterator<Item = &CallFrame> {
        self.frames.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_type_display() {
        assert_eq!(CallType::Sourced.to_string(), "sourced");
        assert_eq!(CallType::Executed.to_string(), "executed");
    }

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

        stack.push(CallType::Sourced, "script1.sh");
        assert!(!stack.is_empty());
        assert_eq!(stack.depth(), 1);

        stack.push(CallType::Executed, "script2.sh");
        assert_eq!(stack.depth(), 2);

        let frame = stack.pop().unwrap();
        assert_eq!(frame.source, "script2.sh");
        assert!(matches!(frame.call_type, CallType::Executed));
        assert_eq!(stack.depth(), 1);

        let frame = stack.pop().unwrap();
        assert_eq!(frame.source, "script1.sh");
        assert!(matches!(frame.call_type, CallType::Sourced));
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

        stack.push(CallType::Executed, "script1.sh");
        assert!(!stack.in_sourced_script());

        stack.push(CallType::Sourced, "script2.sh");
        assert!(stack.in_sourced_script());

        stack.pop();
        assert!(!stack.in_sourced_script());
    }

    #[test]
    fn test_call_stack_iter() {
        let mut stack = CallStack::new();
        stack.push(CallType::Sourced, "script1.sh");
        stack.push(CallType::Executed, "script2.sh");
        stack.push(CallType::Sourced, "script3.sh");

        let frames: Vec<_> = stack.iter().collect();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].source, "script3.sh");
        assert_eq!(frames[1].source, "script2.sh");
        assert_eq!(frames[2].source, "script1.sh");
    }

    #[test]
    fn test_call_stack_display_empty() {
        let stack = CallStack::new();
        assert_eq!(stack.to_string(), "");
    }

    #[test]
    fn test_call_stack_display_with_frames() {
        let mut stack = CallStack::new();
        stack.push(CallType::Sourced, "script1.sh");
        stack.push(CallType::Executed, "script2.sh");

        let output = stack.to_string();
        assert!(output.contains("Script call stack (most recent first):"));
        assert!(output.contains("#0| script2.sh (executed)"));
        assert!(output.contains("#1| script1.sh (sourced)"));
    }

    #[test]
    fn test_call_frame_clone() {
        let frame1 = CallFrame {
            call_type: CallType::Sourced,
            source: "test.sh".to_string(),
        };
        let frame2 = frame1.clone();

        assert_eq!(frame1.source, frame2.source);
        assert!(matches!(frame1.call_type, CallType::Sourced));
        assert!(matches!(frame2.call_type, CallType::Sourced));
    }

    #[test]
    fn test_call_stack_clone() {
        let mut stack1 = CallStack::new();
        stack1.push(CallType::Sourced, "script1.sh");
        stack1.push(CallType::Executed, "script2.sh");

        let stack2 = stack1.clone();
        assert_eq!(stack1.depth(), stack2.depth());

        let frames1: Vec<_> = stack1.iter().map(|f| &f.source).collect();
        let frames2: Vec<_> = stack2.iter().map(|f| &f.source).collect();
        assert_eq!(frames1, frames2);
    }

    #[test]
    fn test_push_with_string_types() {
        let mut stack = CallStack::new();

        // Test with &str
        stack.push(CallType::Sourced, "script1.sh");

        // Test with String
        stack.push(CallType::Executed, String::from("script2.sh"));

        // Test with owned string reference
        let owned = "script3.sh".to_string();
        stack.push(CallType::Sourced, &owned);

        assert_eq!(stack.depth(), 3);
    }
}
