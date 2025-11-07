use clap::{Parser, ValueEnum};
use std::{io::Write, str::FromStr as _, sync::Arc};
use strum::IntoEnumIterator;
use tokio::sync::Mutex;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins, error, interfaces, sys, trace_categories,
};

/// Identifier for a keymap
#[derive(Clone, ValueEnum)]
enum BindKeyMap {
    #[clap(name = "emacs-standard", alias = "emacs")]
    EmacsStandard,
    #[clap(name = "emacs-meta")]
    EmacsMeta,
    #[clap(name = "emacs-ctlx")]
    EmacsCtlx,
    #[clap(name = "vi-command", aliases = &["vi", "vi-move"])]
    ViCommand,
    #[clap(name = "vi-insert")]
    ViInsert,
}

impl BindKeyMap {
    const fn is_vi(&self) -> bool {
        matches!(self, Self::ViCommand | Self::ViInsert)
    }

    #[expect(dead_code)]
    const fn is_emacs(&self) -> bool {
        matches!(
            self,
            Self::EmacsStandard | Self::EmacsMeta | Self::EmacsCtlx
        )
    }
}

/// Inspect and modify key bindings and other input configuration.
#[derive(Parser)]
pub(crate) struct BindCommand {
    /// Name of key map to use.
    #[arg(short = 'm')]
    keymap: Option<BindKeyMap>,
    /// List functions.
    #[arg(short = 'l')]
    list_funcs: bool,
    /// List functions and bindings.
    #[arg(short = 'P')]
    list_funcs_and_bindings: bool,
    /// List functions and bindings in a format suitable for use as input.
    #[arg(short = 'p')]
    list_funcs_and_bindings_reusable: bool,
    /// List key sequences that invoke macros.
    #[arg(short = 'S')]
    list_key_seqs_that_invoke_macros: bool,
    /// List key sequences that invoke macros in a format suitable for use as input.
    #[arg(short = 's')]
    list_key_seqs_that_invoke_macros_reusable: bool,
    /// List variables.
    #[arg(short = 'V')]
    list_vars: bool,
    /// List variables in a format suitable for use as input.
    #[arg(short = 'v')]
    list_vars_reusable: bool,
    /// Find the keys bound to the given named function.
    #[arg(short = 'q', value_name = "FUNC_NAME")]
    query_func_bindings: Option<String>,
    /// Remove all bindings for the given named function.
    #[arg(short = 'u', value_name = "FUNC_NAME")]
    remove_func_bindings: Option<String>,
    /// Remove the binding for the given key sequence.
    #[arg(short = 'r', value_name = "KEY_SEQ")]
    remove_key_seq_binding: Option<String>,
    /// Import bindings from the given file.
    #[arg(short = 'f', value_name = "PATH")]
    bindings_file: Option<String>,
    /// Bind key sequence to command.
    #[arg(short = 'x', value_name = "BINDING")]
    key_seq_bindings: Vec<String>,
    /// List key sequence bindings.
    #[arg(short = 'X')]
    list_key_seq_bindings: bool,
    /// Key sequence binding to readline function or command.
    key_sequence: Option<String>,
}

const BIND_FEATURE_ISSUE_ID: u32 = 380;

impl builtins::Command for BindCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(key_bindings) = context.shell.key_bindings() {
            Ok(self.execute_impl(key_bindings, &context).await?)
        } else {
            writeln!(
                context.stderr(),
                "bind: key bindings not supported in this config"
            )?;

            Ok(ExecutionExitCode::Unimplemented.into())
        }
    }
}

impl BindCommand {
    async fn execute_impl(
        &self,
        bindings: &Arc<Mutex<dyn interfaces::KeyBindings>>,
        context: &brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        let mut bindings = bindings.lock().await;

        if self.list_funcs {
            for func in interfaces::InputFunction::iter() {
                writeln!(context.stdout(), "{func}")?;
            }
        }

        if self.list_funcs_and_bindings {
            for (seq, action) in &bindings.get_current() {
                writeln!(context.stdout(), "{action} can be found on {seq}")?;
            }

            return error::unimp_with_issue("bind -P", BIND_FEATURE_ISSUE_ID);
        }

        if self.list_funcs_and_bindings_reusable {
            return error::unimp_with_issue("bind -p", BIND_FEATURE_ISSUE_ID);
        }

        if self.list_key_seqs_that_invoke_macros {
            return error::unimp_with_issue("bind -S", BIND_FEATURE_ISSUE_ID);
        }

        if self.list_key_seqs_that_invoke_macros_reusable {
            return error::unimp_with_issue("bind -s", BIND_FEATURE_ISSUE_ID);
        }

        if self.list_vars {
            return error::unimp_with_issue("bind -V", BIND_FEATURE_ISSUE_ID);
        }

        if self.list_vars_reusable {
            // For now we'll just display a few items and show defaults.
            writeln!(context.stdout(), "set mark-directories on")?;
            writeln!(context.stdout(), "set mark-symlinked-directories off")?;
        }

        if self.query_func_bindings.is_some() {
            return error::unimp_with_issue("bind -q", BIND_FEATURE_ISSUE_ID);
        }

        if self.remove_func_bindings.is_some() {
            return error::unimp_with_issue("bind -u", BIND_FEATURE_ISSUE_ID);
        }

        if self.remove_key_seq_binding.is_some() {
            return error::unimp_with_issue("bind -r", BIND_FEATURE_ISSUE_ID);
        }

        if self.bindings_file.is_some() {
            return error::unimp_with_issue("bind -f", BIND_FEATURE_ISSUE_ID);
        }

        if self.list_key_seq_bindings {
            return error::unimp_with_issue("bind -X", BIND_FEATURE_ISSUE_ID);
        }

        if !self.key_seq_bindings.is_empty() {
            if self.keymap.as_ref().is_some_and(|k| k.is_vi()) {
                // NOTE(vi): Quietly ignore since we don't support vi mode.
                return Ok(ExecutionResult::success());
            }

            for key_seq_and_command in &self.key_seq_bindings {
                let (key_seq, command) = parse_key_sequence_and_shell_command(key_seq_and_command)?;
                bind_key_sequence_to_shell_cmd(&mut *bindings, key_seq, command)?;
            }
        }

        if let Some(key_sequence) = &self.key_sequence {
            if self.keymap.as_ref().is_some_and(|k| k.is_vi()) {
                // NOTE(vi): Quietly ignore since we don't support vi mode.
                return Ok(ExecutionResult::success());
            }

            let (key_seq, target) = parse_key_sequence_and_readline_target(key_sequence.as_str())?;
            bind_key_sequence_to_readline_target(&mut *bindings, key_seq, target)?;
        }

        drop(bindings);

        Ok(ExecutionResult::success())
    }
}

fn parse_key_sequence_and_shell_command(
    input: &str,
) -> Result<(interfaces::KeySequence, String), brush_core::Error> {
    tracing::debug!(target: trace_categories::INPUT,
        "parsing key binding entry: '{input}'"
    );

    // First trim any whitespace.
    let input = input.trim();

    // This should be something of the form:
    //     "KEY-SEQUENCE": SHELL-COMMAND
    let binding = brush_parser::readline_binding::parse_key_sequence_shell_cmd_binding(input)?;
    let strokes = key_sequence_to_abstract_strokes(&binding.seq)?;

    Ok((interfaces::KeySequence { strokes }, binding.shell_cmd))
}

#[derive(Debug)]
#[allow(dead_code, reason = "not all variants implemented yet")]
enum BindableReadlineTarget {
    Function(brush_core::interfaces::InputFunction),
    Command(String),
}

fn parse_key_sequence_and_readline_target(
    input: &str,
) -> Result<(interfaces::KeySequence, BindableReadlineTarget), brush_core::Error> {
    tracing::debug!(target: trace_categories::INPUT,
        "parsing key binding entry: '{input}'"
    );

    // First trim any whitespace.
    let input = input.trim();

    // This should be of one of these forms:
    //     "KEY-SEQUENCE":function-name
    //     "KEY-SEQUENCE":readline-command
    let binding = brush_parser::readline_binding::parse_key_sequence_readline_binding(input)?;
    let strokes = key_sequence_to_abstract_strokes(&binding.seq)?;

    match binding.target {
        brush_parser::readline_binding::ReadlineTarget::Function(func_name) => {
            let func = parse_readline_function(func_name.as_str())?;
            Ok((
                interfaces::KeySequence { strokes },
                BindableReadlineTarget::Function(func),
            ))
        }
        brush_parser::readline_binding::ReadlineTarget::Command(cmd) => Ok((
            interfaces::KeySequence { strokes },
            BindableReadlineTarget::Command(cmd),
        )),
    }
}

fn bind_key_sequence_to_shell_cmd(
    bindings: &mut dyn interfaces::KeyBindings,
    key_sequence: interfaces::KeySequence,
    command: String,
) -> Result<(), brush_core::Error> {
    tracing::debug!(target: trace_categories::INPUT,
        "binding key sequence: '{key_sequence}' => command '{command}'"
    );

    bindings.bind(key_sequence, interfaces::KeyAction::ShellCommand(command))?;

    Ok(())
}

fn bind_key_sequence_to_readline_target(
    bindings: &mut dyn interfaces::KeyBindings,
    key_sequence: interfaces::KeySequence,
    target: BindableReadlineTarget,
) -> Result<(), brush_core::Error> {
    match target {
        BindableReadlineTarget::Function(func) => {
            tracing::debug!(target: trace_categories::INPUT,
                "binding key sequence: '{key_sequence}' => readline function '{func}'"
            );

            if matches!(func, interfaces::InputFunction::ViEditingMode) {
                // NOTE(vi): We don't support vi mode; silently ignore.
                return Ok(());
            }

            bindings.bind(key_sequence, interfaces::KeyAction::DoInputFunction(func))?;
            Ok(())
        }
        BindableReadlineTarget::Command(cmd_macro) => {
            tracing::debug!(target: trace_categories::INPUT,
                "binding key sequence: '{key_sequence}' => readline macro '{cmd_macro}'"
            );

            error::unimp("binding key sequence to readline macro")
        }
    }
}

fn key_sequence_to_abstract_strokes(
    seq: &brush_parser::readline_binding::KeySequence,
) -> Result<Vec<interfaces::KeyStroke>, brush_core::Error> {
    let phys_strokes = brush_parser::readline_binding::key_sequence_to_strokes(seq)?;

    // Lift from key codes to abstract keys.
    let mut abstract_strokes = vec![];
    for mut phys_stroke in phys_strokes {
        let mut key = sys::input::try_get_key_from_key_code(phys_stroke.key_code.as_slice());

        // If we couldn't interpret it directly but we see it starts with the escape character,
        // try to see if we can parse it as an Alt+<key> sequence.
        if key.is_none() && phys_stroke.key_code.len() > 1 && phys_stroke.key_code[0] == b'\x1b' {
            key = sys::input::try_get_key_from_key_code(&phys_stroke.key_code[1..]);
            if key.is_some() {
                phys_stroke.meta = true;
            }
        }

        let Some(key) = key else {
            return Err(error::ErrorKind::UnhandledKeyCode(phys_stroke.key_code).into());
        };

        abstract_strokes.push(interfaces::KeyStroke {
            alt: phys_stroke.meta,
            control: phys_stroke.control,
            shift: false,
            key,
        });
    }

    Ok(abstract_strokes)
}

fn parse_readline_function(
    func_name: &str,
) -> Result<interfaces::InputFunction, brush_core::Error> {
    interfaces::InputFunction::from_str(func_name).map_err(|_err| {
        brush_core::ErrorKind::UnknownKeyBindingFunction(func_name.to_owned()).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::{assert_eq, assert_matches};

    #[test]
    fn parse_example_key_sequence_and_readline_func() {
        let (key_seq, target) =
            parse_key_sequence_and_readline_target(r#""\C-a":beginning-of-line"#).unwrap();

        assert_eq!(
            key_seq,
            interfaces::KeySequence {
                strokes: vec![interfaces::KeyStroke {
                    alt: false,
                    control: true,
                    shift: false,
                    key: interfaces::Key::Character('a'),
                }],
            }
        );

        assert_matches!(
            target,
            BindableReadlineTarget::Function(interfaces::InputFunction::BeginningOfLine)
        );
    }

    #[test]
    fn parse_escape_char_key_binding() {
        let (key_seq, target) =
            parse_key_sequence_and_readline_target(r#""\er":transpose-chars"#).unwrap();

        assert_eq!(
            key_seq,
            interfaces::KeySequence {
                strokes: vec![interfaces::KeyStroke {
                    alt: true,
                    control: false,
                    shift: false,
                    key: interfaces::Key::Character('r'),
                }],
            }
        );

        assert_matches!(
            target,
            BindableReadlineTarget::Function(interfaces::InputFunction::TransposeChars)
        );
    }
}
