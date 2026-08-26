//! `bind` builtin: `BindCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]
#![allow(dead_code, reason = "transplanted helpers awaiting wiring")]

use bpaf::Parser;
use itertools::Itertools as _;
use std::{collections::HashMap, io::Write, str::FromStr};
use strum::IntoEnumIterator;
use super::BindError;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;
use brush_core::interfaces;
use brush_core::sys;
use brush_core::interfaces::KeyAction;
use brush_core::trace_categories;

/// Identifier for a keymap
#[derive(Clone)]
pub(crate) enum BindKeyMap {
    EmacsStandard,
    EmacsMeta,
    EmacsCtlx,
    ViCommand,
    ViInsert,
}
impl std::str::FromStr for BindKeyMap {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "emacs-standard" | "emacs" => Ok(Self::EmacsStandard),
            "emacs-meta" => Ok(Self::EmacsMeta),
            "emacs-ctlx" => Ok(Self::EmacsCtlx),
            "vi-command" | "vi" | "vi-move" => Ok(Self::ViCommand),
            "vi-insert" => Ok(Self::ViInsert),
            _ => Err(format!("invalid keymap: {s}")),
        }
    }
}

fn parse_key_sequence(input: &str) -> Result<brush_core::interfaces::KeySequence, BindError> {
    // First trim any whitespace.
    let input = input.trim();

    let parsed = brush_parser::readline_binding::parse_key_sequence(input)?;
    let abstract_seq = key_sequence_to_abstract_strokes(&parsed)?;

    Ok(abstract_seq)
}

fn parse_key_sequence_and_shell_command(
    input: &str,
) -> Result<(brush_core::interfaces::KeySequence, String), BindError> {
    tracing::debug!(target: trace_categories::INPUT,
        "parsing key binding entry: '{input}'"
    );

    // First trim any whitespace.
    let input = input.trim();

    // This should be something of the form:
    //     "KEY-SEQUENCE": SHELL-COMMAND
    let binding = brush_parser::readline_binding::parse_key_sequence_shell_cmd_binding(input)?;
    let abstract_seq = key_sequence_to_abstract_strokes(&binding.seq)?;

    Ok((abstract_seq, binding.shell_cmd))
}

#[derive(Debug)]
#[allow(dead_code, reason = "not all variants implemented yet")]
enum BindableReadlineTarget {
    Function(brush_core::interfaces::InputFunction),
    Macro(brush_core::interfaces::KeySequence),
}

fn parse_key_sequence_and_readline_target(
    input: &str,
) -> Result<(brush_core::interfaces::KeySequence, BindableReadlineTarget), BindError> {
    tracing::debug!(target: trace_categories::INPUT,
        "parsing key binding entry: '{input}'"
    );

    // First trim any whitespace.
    let input = input.trim();

    // This should be of one of these forms:
    //     "KEY-SEQUENCE":function-name
    //     "KEY-SEQUENCE":readline-command
    let binding = brush_parser::readline_binding::parse_key_sequence_readline_binding(input)?;
    let abstract_seq = key_sequence_to_abstract_strokes(&binding.seq)?;

    match binding.target {
        brush_parser::readline_binding::ReadlineTarget::Function(func_name) => {
            let func = parse_readline_function(func_name.as_str())?;
            Ok((abstract_seq, BindableReadlineTarget::Function(func)))
        }
        brush_parser::readline_binding::ReadlineTarget::Macro(target_seq_str) => {
            let parsed_target =
                brush_parser::readline_binding::parse_key_sequence(&target_seq_str)?;
            let abstract_target = key_sequence_to_abstract_strokes(&parsed_target)?;
            Ok((abstract_seq, BindableReadlineTarget::Macro(abstract_target)))
        }
    }
}

fn bind_key_sequence_to_shell_cmd(
    bindings: &mut dyn interfaces::KeyBindings,
    key_sequence: brush_core::interfaces::KeySequence,
    command: String,
) -> Result<(), BindError> {
    tracing::debug!(target: trace_categories::INPUT,
        "binding key sequence: '{key_sequence}' => command '{command}'"
    );

    bindings.bind(key_sequence, interfaces::KeyAction::ShellCommand(command))?;

    Ok(())
}

fn bind_key_sequence_to_readline_target(
    bindings: &mut dyn interfaces::KeyBindings,
    key_sequence: brush_core::interfaces::KeySequence,
    target: BindableReadlineTarget,
) -> Result<(), BindError> {
    match target {
        BindableReadlineTarget::Function(func) => {
            tracing::debug!(target: trace_categories::INPUT,
                "binding key sequence: '{key_sequence}' => readline function '{func}'"
            );

            if matches!(func, brush_core::interfaces::InputFunction::ViEditingMode) {
                // NOTE(vi): We don't support vi mode; silently ignore.
                return Ok(());
            }

            bindings.bind(key_sequence, interfaces::KeyAction::DoInputFunction(func))?;
            Ok(())
        }
        BindableReadlineTarget::Macro(cmd_macro) => {
            tracing::debug!(target: trace_categories::INPUT,
                "binding key sequence: '{key_sequence}' => readline macro '{cmd_macro}'"
            );

            bindings.define_macro(key_sequence, cmd_macro)?;
            Ok(())
        }
    }
}

fn key_sequence_to_abstract_strokes(
    seq: &brush_parser::readline_binding::KeySequence,
) -> Result<brush_core::interfaces::KeySequence, BindError> {
    let phys_strokes = brush_parser::readline_binding::key_sequence_to_strokes(seq)?;

    // Lift from key codes to abstract keys.
    let mut abstract_strokes = vec![];
    let mut key_code_bytes = vec![];
    let mut uninterpretable = false;
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

        // When storing as bytes, apply control modifier to the key code.
        let mut raw_bytes = phys_stroke.key_code.clone();
        if phys_stroke.control {
            for byte in &mut raw_bytes {
                // Control characters are computed by ANDing with 0x1F
                *byte &= 0x1F;
            }
        }
        key_code_bytes.push(raw_bytes);

        if let Some(key) = key {
            abstract_strokes.push(interfaces::KeyStroke {
                alt: phys_stroke.meta,
                control: phys_stroke.control,
                shift: false,
                key,
            });
        } else {
            uninterpretable = true;
        }
    }

    if uninterpretable {
        Ok(brush_core::interfaces::KeySequence::Bytes(key_code_bytes))
    } else {
        Ok(brush_core::interfaces::KeySequence::Strokes(abstract_strokes))
    }
}

fn parse_readline_function(func_name: &str) -> Result<brush_core::interfaces::InputFunction, BindError> {
    brush_core::interfaces::InputFunction::from_str(func_name)
        .map_err(|_err| BindError::UnknownKeyBindingFunction(func_name.to_owned()))
}

const fn to_onoff(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn display_funcs_and_bindings(
    bindings: &dyn interfaces::KeyBindings,
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    reusable: bool,
) -> Result<(), BindError> {
    let mut sequences_by_func: HashMap<brush_core::interfaces::InputFunction, Vec<brush_core::interfaces::KeySequence>> = HashMap::new();
    for (seq, action) in &bindings.get_current() {
        let KeyAction::DoInputFunction(func) = action else {
            continue;
        };

        sequences_by_func
            .entry(func.clone())
            .or_default()
            .push(seq.clone());
    }

    let sorted_funcs = brush_core::interfaces::InputFunction::iter().sorted_by_key(|f| f.to_string());

    for func in sorted_funcs {
        match sequences_by_func.get(&func) {
            Some(seqs) if reusable => {
                for seq in seqs {
                    writeln!(context.stdout(), "\"{seq}\": {func}")?;
                }
            }
            Some(seqs) => {
                writeln!(
                    context.stdout(),
                    "{func} can be found on {}.",
                    seqs.iter().map(|seq| std::format!("\"{seq}\"")).join(", ")
                )?;
            }
            None if reusable => {
                writeln!(context.stdout(), "# {func} (not bound)")?;
            }
            None => {
                writeln!(context.stdout(), "{func} is not bound to any keys")?;
            }
        }
    }

    Ok(())
}

fn display_macros(
    bindings: &dyn interfaces::KeyBindings,
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    reusable: bool,
) -> Result<(), BindError> {
    for (left, right) in bindings.get_macros() {
        if reusable {
            writeln!(context.stdout(), "\"{left}\": \"{right}\"")?;
        } else {
            writeln!(context.stdout(), "{left} outputs {right}")?;
        }
    }

    Ok(())
}

fn find_key_seqs_bound_to_function(
    bindings: &dyn interfaces::KeyBindings,
    func_str: &str,
) -> Result<Vec<brush_core::interfaces::KeySequence>, BindError> {
    let Ok(func_to_find) = brush_core::interfaces::InputFunction::from_str(func_str) else {
        return Err(BindError::UnknownFunction(func_str.to_owned()));
    };

    let mut found_seqs = vec![];

    for (seq, action) in &bindings.get_current() {
        if let KeyAction::DoInputFunction(func) = action
            && *func == func_to_find
        {
            found_seqs.push(seq.clone());
        }
    }

    Ok(found_seqs)
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
            brush_core::interfaces::KeySequence::Strokes(vec![interfaces::KeyStroke {
                alt: false,
                control: true,
                shift: false,
                key: interfaces::Key::Character('a'),
            }])
        );

        assert_matches!(
            target,
            BindableReadlineTarget::Function(brush_core::interfaces::InputFunction::BeginningOfLine)
        );
    }

    #[test]
    fn parse_escape_char_key_binding() {
        let (key_seq, target) =
            parse_key_sequence_and_readline_target(r#""\er":transpose-chars"#).unwrap();

        assert_eq!(
            key_seq,
            brush_core::interfaces::KeySequence::Strokes(vec![interfaces::KeyStroke {
                alt: true,
                control: false,
                shift: false,
                key: interfaces::Key::Character('r'),
            }])
        );

        assert_matches!(
            target,
            BindableReadlineTarget::Function(brush_core::interfaces::InputFunction::TransposeChars)
        );
    }
}

/// Inspect and modify key bindings and other input configuration.
pub(crate) struct BindCommand {
    pub(super) keymap: Option<BindKeyMap>,
    pub(super) list_funcs: bool,
    pub(super) list_funcs_and_bindings: bool,
    pub(super) list_funcs_and_bindings_reusable: bool,
    pub(super) list_key_seqs_that_invoke_macros: bool,
    pub(super) list_key_seqs_that_invoke_macros_reusable: bool,
    pub(super) list_vars: bool,
    pub(super) list_vars_reusable: bool,
    pub(super) query_func_bindings: Option<String>,
    pub(super) remove_func_bindings: Option<String>,
    pub(super) remove_key_seq_binding: Option<String>,
    pub(super) bindings_file: Option<String>,
    pub(super) key_seq_bindings: Vec<String>,
    pub(super) list_key_seq_bindings: bool,
    pub(super) key_sequence: Option<String>,
}

impl crate::args::bpaf_support::BpafArgs for BindCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let keymap = bpaf::short('m')
            .help("Name of key map to use.")
            .argument::<BindKeyMap>("KEYMAP")
            .optional();
        let list_funcs = bpaf::short('l').help("List functions.").switch();
        let list_funcs_and_bindings = bpaf::short('P')
            .help("List functions and bindings.")
            .switch();
        let list_funcs_and_bindings_reusable = bpaf::short('p')
            .help("List functions and bindings in a format suitable for use as input.")
            .switch();
        let list_key_seqs_that_invoke_macros = bpaf::short('S')
            .help("List key sequences that invoke macros.")
            .switch();
        let list_key_seqs_that_invoke_macros_reusable = bpaf::short('s')
            .help("List key sequences that invoke macros in a format suitable for use as input.")
            .switch();
        let list_vars = bpaf::short('V').help("List variables.").switch();
        let list_vars_reusable = bpaf::short('v')
            .help("List variables in a format suitable for use as input.")
            .switch();
        let query_func_bindings = bpaf::short('q')
            .help("Find the keys bound to the given named function.")
            .argument::<String>("FUNC_NAME")
            .optional();
        let remove_func_bindings = bpaf::short('u')
            .help("Remove all bindings for the given named function.")
            .argument::<String>("FUNC_NAME")
            .optional();
        let remove_key_seq_binding = bpaf::short('r')
            .help("Remove the binding for the given key sequence.")
            .argument::<String>("KEY_SEQ")
            .optional();
        let bindings_file = bpaf::short('f')
            .help("Import bindings from the given file.")
            .argument::<String>("PATH")
            .optional();
        let key_seq_bindings = bpaf::short('x')
            .help("Bind key sequence to command.")
            .argument::<String>("BINDING")
            .many();
        let list_key_seq_bindings = bpaf::short('X')
            .help("List key sequence bindings.")
            .switch();
        let key_sequence = bpaf::positional::<String>("KEY_SEQUENCE")
            .help("Key sequence binding to readline function or command.")
            .optional();

        bpaf::construct!(BindCommand {
            keymap,
            list_funcs,
            list_funcs_and_bindings,
            list_funcs_and_bindings_reusable,
            list_key_seqs_that_invoke_macros,
            list_key_seqs_that_invoke_macros_reusable,
            list_vars,
            list_vars_reusable,
            query_func_bindings,
            remove_func_bindings,
            remove_key_seq_binding,
            bindings_file,
            key_seq_bindings,
            list_key_seq_bindings,
            key_sequence,
        })
    }
fn about() -> &'static str {
        "Inspect and modify key bindings and other input configuration."
    }
fn synopsis() -> &'static str {
        "[-lpsPSVX] [-m KEYMAP] [-q|-u|-r ARG] [-f PATH] [-x BINDING]... [KEY_SEQUENCE]"
    }
}

impl FromArgs for BindCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for BindCommand {
    type Error = BindError;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
