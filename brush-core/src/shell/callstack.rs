//! Call stack management for the shell.

use crate::{callstack, env, error, functions, trace_categories};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns whether or not the shell is actively executing in a sourced script.
    pub fn in_sourced_script(&self) -> bool {
        self.call_stack.in_sourced_script()
    }

    /// Returns whether or not the shell is actively executing in a shell function.
    pub fn in_function(&self) -> bool {
        self.call_stack.in_function()
    }

    /// Returns how many loops enclose the command the shell is running: the loops that
    /// `break` and `continue` can leave.
    ///
    /// A function's body starts outside its caller's loops, as does a compound command
    /// that runs in a subshell of its own (`( ... )`, a pipeline stage, or an
    /// asynchronous list). A sourced script, `eval`, a trap handler, a command or process
    /// substitution, and a simple command run in a subshell share the loops they're run
    /// in.
    pub const fn loop_depth(&self) -> usize {
        self.loop_depth
    }

    /// Enters a loop -- its condition and its body -- on the shell, until the returned
    /// guard is dropped.
    pub(crate) const fn enter_loop(&mut self) -> ShellGuard<'_, SE> {
        let loop_depth = self.loop_depth;
        self.loop_depth += 1;
        ShellGuard::new(self, Leave::Loop { loop_depth })
    }

    /// Starts this shell, a subshell of its own running a compound command, outside the
    /// loops it was created in.
    pub(crate) const fn start_outside_loops(&mut self) {
        self.loop_depth = 0;
    }

    /// Updates the shell's internal tracking state to reflect that a new interactive
    /// session is being started.
    pub fn start_interactive_session(&mut self) -> Result<(), error::Error> {
        self.call_stack.push_interactive_session();
        Ok(())
    }

    /// Updates the shell's internal tracking state to reflect that the current
    /// interactive session is ending.
    pub fn end_interactive_session(&mut self) -> Result<(), error::Error> {
        if self
            .call_stack
            .current_frame()
            .is_none_or(|frame| !frame.frame_type.is_interactive_session())
        {
            return Err(error::ErrorKind::NotInInteractiveSession.into());
        }

        self.call_stack.pop();

        Ok(())
    }

    /// Updates the shell's internal tracking state to reflect that command
    /// string mode is being started.
    pub fn start_command_string_mode(&mut self) {
        self.call_stack.push_command_string();
    }

    /// Updates the shell's internal tracking state to reflect that command
    /// string mode is ending.
    pub fn end_command_string_mode(&mut self) -> Result<(), error::Error> {
        if self
            .call_stack
            .current_frame()
            .is_none_or(|frame| !frame.frame_type.is_command_string())
        {
            return Err(error::ErrorKind::NotExecutingCommandString.into());
        }

        self.call_stack.pop();

        Ok(())
    }

    /// Enters a trap handler for `signal` on the shell, until the returned guard is dropped.
    /// Leaving it restores `$?`, so the handler doesn't change the status that triggered it.
    pub(crate) fn enter_trap_handler(
        &mut self,
        signal: crate::traps::TrapSignal,
        handler: Option<&crate::traps::TrapHandler>,
    ) -> ShellGuard<'_, SE> {
        let last_exit_status = self.last_exit_status;
        self.call_stack.push_trap_handler(signal, handler);
        let frames = self.call_stack.depth();
        ShellGuard::new(
            self,
            Leave::TrapHandler {
                frames,
                last_exit_status,
            },
        )
    }

    /// Enters a script on the shell, until the returned guard is dropped.
    pub(crate) fn enter_script(
        &mut self,
        call_type: callstack::ScriptCallType,
        source_info: &crate::SourceInfo,
        args: impl IntoIterator<Item = String>,
    ) -> ShellGuard<'_, SE> {
        self.call_stack.push_script(call_type, source_info, args);
        let frames = self.call_stack.depth();
        ShellGuard::new(self, Leave::Script { frames })
    }

    /// Enters a call to the shell function `name` on the shell, with its own scope for local
    /// variables, until the returned guard is dropped.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being entered.
    /// * `function` - The function being entered.
    /// * `args` - The arguments being passed to the function.
    pub(crate) fn enter_function(
        &mut self,
        name: &str,
        function: &functions::Registration,
        args: impl IntoIterator<Item = String>,
    ) -> Result<ShellGuard<'_, SE>, error::Error> {
        if let Some(max_call_depth) = self.options.max_function_call_depth
            && self.call_stack.function_call_depth() >= max_call_depth
        {
            return Err(error::ErrorKind::MaxFunctionCallDepthExceeded.into());
        }

        if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
            let depth = self.call_stack.function_call_depth();
            let prefix = repeated_char_str(' ', depth);
            tracing::debug!(target: trace_categories::FUNCTIONS, "Entering func [depth={depth}]: {prefix}{name}");
        }

        self.call_stack.push_function(name, function, args);
        self.env.push_scope(env::EnvironmentScope::Local);
        let leave = Leave::Function {
            frames: self.call_stack.depth(),
            scopes: self.env.scope_depth(),
            loop_depth: self.loop_depth,
        };

        // The function's body starts outside its caller's loops.
        self.loop_depth = 0;

        Ok(ShellGuard::new(self, leave))
    }

    /// Enters a new variable scope on the shell, until the returned guard is dropped.
    pub(crate) fn enter_scope(&mut self, scope: env::EnvironmentScope) -> ShellGuard<'_, SE> {
        self.env.push_scope(scope);
        let scopes = self.env.scope_depth();
        ShellGuard::new(self, Leave::Scope { scopes, scope })
    }

    /// Blocks the delivery of traps until the returned guard is dropped. Blocks nest: traps
    /// are delivered again only once every block is dropped.
    pub(crate) const fn block_trap_delivery(&mut self) -> ShellGuard<'_, SE> {
        self.call_stack.acquire_trap_delivery_block();
        ShellGuard::new(self, Leave::TrapBlock)
    }

    /// Returns the *current* positional arguments for the shell ($1 and beyond).
    /// Influenced by the current call stack.
    pub fn current_shell_args(&self) -> &[String] {
        for frame in self.call_stack.iter() {
            match frame.frame_type {
                // Function calls always shadow positional parameters.
                crate::callstack::FrameType::Function(..) => return &frame.args,
                // Executed scripts always shadow positional parameters.
                _ if frame.frame_type.is_run_script() => return &frame.args,
                // Sourced scripts shadow positional parameters if they have arguments.
                _ if frame.frame_type.is_sourced_script() && !frame.args.is_empty() => {
                    return &frame.args;
                }
                _ => (),
            }
        }

        self.args.as_slice()
    }

    /// Returns a mutable reference to *current* positional parameters for the shell
    /// ($1 and beyond).
    pub fn current_shell_args_mut(&mut self) -> &mut Vec<String> {
        for frame in self.call_stack.iter_mut() {
            match frame.frame_type {
                // Function calls always shadow positional parameters.
                crate::callstack::FrameType::Function(..) => return &mut frame.args,
                // Executed scripts always shadow positional parameters.
                _ if frame.frame_type.is_run_script() => return &mut frame.args,
                // Sourced scripts shadow positional parameters if they have arguments.
                _ if frame.frame_type.is_sourced_script() && !frame.args.is_empty() => {
                    return &mut frame.args;
                }
                _ => (),
            }
        }

        &mut self.args
    }
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}

/// Guards something entered on a shell -- a function call, a script, a trap handler, a
/// loop, a variable scope, or a block on trap delivery -- and leaves it when dropped: when
/// whatever runs in it returns, fails, or is cancelled (its future dropped, as when the
/// user hits Ctrl-C during completion).
///
/// It dereferences to the shell, for running whatever's entered.
pub(crate) struct ShellGuard<'a, SE: crate::extensions::ShellExtensions> {
    shell: &'a mut crate::Shell<SE>,
    leave: Leave,
}

/// What a [`ShellGuard`] leaves when it's dropped, with how deep the call stack (`frames`)
/// and variable scopes (`scopes`) were once it was entered, and how many loops enclosed
/// what entered it (`loop_depth`).
#[derive(Clone, Copy, Debug)]
enum Leave {
    Function {
        frames: usize,
        scopes: usize,
        loop_depth: usize,
    },
    Loop {
        loop_depth: usize,
    },
    Script {
        frames: usize,
    },
    TrapHandler {
        frames: usize,
        last_exit_status: u8,
    },
    Scope {
        scopes: usize,
        scope: env::EnvironmentScope,
    },
    TrapBlock,
}

impl<'a, SE: crate::extensions::ShellExtensions> ShellGuard<'a, SE> {
    const fn new(shell: &'a mut crate::Shell<SE>, leave: Leave) -> Self {
        Self { shell, leave }
    }
}

impl<SE: crate::extensions::ShellExtensions> std::ops::Deref for ShellGuard<'_, SE> {
    type Target = crate::Shell<SE>;

    fn deref(&self) -> &Self::Target {
        self.shell
    }
}

impl<SE: crate::extensions::ShellExtensions> std::ops::DerefMut for ShellGuard<'_, SE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.shell
    }
}

impl<SE: crate::extensions::ShellExtensions> ShellGuard<'_, SE> {
    /// Returns whether this guard's frame is on top of the call stack, `depth` frames deep.
    fn frame_on_top(&self, depth: usize, is_expected: fn(&callstack::FrameType) -> bool) -> bool {
        let stack = &self.shell.call_stack;
        stack.depth() == depth
            && stack
                .current_frame()
                .is_some_and(|frame| is_expected(&frame.frame_type))
    }

    /// Pops this guard's frame if it's on top, `depth` frames deep; returns whether it was.
    fn pop_frame(&mut self, depth: usize, is_expected: fn(&callstack::FrameType) -> bool) -> bool {
        let on_top = self.frame_on_top(depth, is_expected);
        if on_top {
            self.shell.call_stack.pop();
        }
        on_top
    }

    /// Pops this guard's variable scope if it's on top, `depth` scopes deep; returns whether
    /// it was.
    fn pop_scope(&mut self, depth: usize, scope_type: env::EnvironmentScope) -> bool {
        self.shell.env.scope_depth() == depth && self.shell.env.pop_scope(scope_type).is_ok()
    }

    /// Reports that what ran in this guard didn't leave everything it entered.
    fn report_imbalance(&self) {
        tracing::error!(
            "shell state out of balance when leaving {:?}: call stack {} deep, {} variable scopes, {} loops",
            self.leave,
            self.shell.call_stack.depth(),
            self.shell.env.scope_depth(),
            self.shell.loop_depth
        );
    }
}

impl<SE: crate::extensions::ShellExtensions> Drop for ShellGuard<'_, SE> {
    fn drop(&mut self) {
        // Whatever was entered on top of this has been left already, since its guard
        // borrowed this one; so unless something left the shell out of balance, what this
        // entered is on top. If it isn't, report that and pop nothing, rather than risk
        // popping something else.
        let left = match self.leave {
            Leave::Function {
                frames,
                scopes,
                loop_depth,
            } => {
                self.shell.loop_depth = loop_depth;

                // Pop the frame and its scope together, or neither.
                let left = self.frame_on_top(frames, callstack::FrameType::is_function)
                    && self.pop_scope(scopes, env::EnvironmentScope::Local);
                if left
                    && let Some(frame) = self.shell.call_stack.pop()
                    && let callstack::FrameType::Function(call) = frame.frame_type
                    && tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG)
                {
                    let depth = self.shell.call_stack.function_call_depth();
                    let prefix = repeated_char_str(' ', depth);
                    tracing::debug!(target: trace_categories::FUNCTIONS, "Exiting func  [depth={depth}]: {prefix}{}", call.function_name);
                }
                left
            }
            Leave::Loop { loop_depth } => {
                let left = self.shell.loop_depth == loop_depth + 1;
                self.shell.loop_depth = loop_depth;
                left
            }
            Leave::Script { frames } => self.pop_frame(frames, callstack::FrameType::is_script),
            Leave::TrapHandler {
                frames,
                last_exit_status,
            } => {
                self.shell.last_exit_status = last_exit_status;
                self.pop_frame(frames, callstack::FrameType::is_trap_handler)
            }
            Leave::Scope { scopes, scope } => self.pop_scope(scopes, scope),
            Leave::TrapBlock => {
                let blocked = self.shell.call_stack.is_trap_delivery_suppressed();
                self.shell.call_stack.release_trap_delivery_block();
                blocked
            }
        };

        if !left {
            self.report_imbalance();
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use super::*;

    /// Guards pop nothing -- not even part of what they entered -- if what ran in them left
    /// the call stack or variable scopes out of balance (as a buggy builtin could).
    #[tokio::test]
    async fn guards_pop_nothing_when_out_of_balance() -> Result<(), error::Error> {
        let mut shell = crate::Shell::builder()
            .profile(crate::ProfileLoadBehavior::Skip)
            .rc(crate::RcLoadBehavior::Skip)
            .build()
            .await?;
        let params = shell.default_exec_params();
        let result = shell
            .run_string("f() { :; }", &crate::SourceInfo::default(), &params)
            .await?;
        assert!(result.is_success());
        let f = shell
            .funcs()
            .get("f")
            .cloned()
            .ok_or_else(|| error::ErrorKind::FunctionNotFound("f".into()))?;
        let frames = shell.call_stack.depth();
        let scopes = shell.env.scope_depth();

        // A function whose scope has another left on top of it.
        let mut function = shell.enter_function("f", &f, std::iter::empty())?;
        function.env_mut().push_scope(env::EnvironmentScope::Local);
        drop(function);
        assert_eq!(shell.call_stack.depth(), frames + 1);
        assert_eq!(shell.env.scope_depth(), scopes + 2);

        // A script whose frame was replaced by another.
        let mut script = shell.enter_script(
            callstack::ScriptCallType::Source,
            &crate::SourceInfo::default(),
            std::iter::empty(),
        );
        script.call_stack.pop();
        script.start_command_string_mode();
        drop(script);
        assert_eq!(shell.call_stack.depth(), frames + 2);

        Ok(())
    }

    /// Loops and function calls leave the loop depth as they found it, however they end.
    #[tokio::test]
    async fn loop_depth_is_restored_however_a_loop_or_function_ends() -> Result<(), error::Error> {
        let mut shell = crate::Shell::builder()
            .profile(crate::ProfileLoadBehavior::Skip)
            .rc(crate::RcLoadBehavior::Skip)
            .build()
            .await?;
        let params = shell.default_exec_params();

        // The shell has no builtins here, so the scripts use none.
        for script in [
            "for i in 1 2; do x=$i; done",
            "until x=1; do x=2; done",
            "for i in 1; do x=$((1 / 0)); done",
            "while x=$((1 / 0)); do x=1; done",
            "f() { for i in 1; do x=$((1 / 0)); done; }; for i in 1; do f; done",
        ] {
            let _ = shell
                .run_string(script, &crate::SourceInfo::default(), &params)
                .await;
            assert_eq!(shell.loop_depth(), 0, "{script}");
        }

        let mut outer = shell.enter_loop();
        let inner = outer.enter_loop();
        assert_eq!(inner.loop_depth(), 2);
        drop(inner);
        assert_eq!(outer.loop_depth(), 1);
        drop(outer);
        assert_eq!(shell.loop_depth(), 0);

        Ok(())
    }
}
