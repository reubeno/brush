//! The usage-rs implementation of the argument-model backend.
//!
//! Builds a `'static` [`usage::argv::Command`] graph from the neutral spec and
//! drives `usage::argv::Parser`'s event stream, mapping flag and positional
//! events back to declaration ids. Help pages render through
//! `usage::argv::help::render_styled`.

use std::ffi::OsStr;

use usage::argv::{Arg, ArgAction, Command, DoubleDash, Error as UsageError, Event, Flag, Parser};

use super::model::{ArgKind, CommandSpec, ParsedValues};
use crate::builtins::BuiltinArgParseError;

/// The usage backend.
pub struct UsageBackend;

fn render_parse_error(
    spec: &usage::argv::spec::Spec<'_>,
    argv: &[&OsStr],
    err: &UsageError<'_, '_>,
) -> BuiltinArgParseError {
    match err {
        UsageError::Help { cmd, long } => BuiltinArgParseError {
            message: usage::argv::help::render_styled(
                spec,
                cmd,
                *long,
                usage::argv::help::Style::auto(),
            )
            .unwrap_or_default(),
            help_request: true,
        },
        UsageError::MissingArgsHelp { cmd } => BuiltinArgParseError {
            message: usage::argv::help::render_styled(
                spec,
                cmd,
                false,
                usage::argv::help::Style::auto_stderr(),
            )
            .unwrap_or_default(),
            help_request: false,
        },
        UsageError::Version { .. } => BuiltinArgParseError {
            message: String::from("Version information requested.\n"),
            help_request: true,
        },
        _ => BuiltinArgParseError {
            message: usage::argv::render_failure(spec, argv, err),
            help_request: false,
        },
    }
}

impl super::ArgParserBackend for UsageBackend {
    fn parse(
        &self,
        spec: &'static CommandSpec,
        name: &str,
        argv: &[String],
    ) -> Result<ParsedValues, BuiltinArgParseError> {
        let command = build_command(spec, name);
        let argv_refs: Vec<&OsStr> = argv.iter().map(|arg| OsStr::new(arg)).collect();

        let file_spec = borrowed_spec(spec, Box::leak(name.to_owned().into_boxed_str()));
        let mut parser = Parser::new(command, &argv_refs);
        let mut values = ParsedValues::new(spec);

        if std::env::var_os("BRUSH_DBG").is_some() {
            std::eprintln!(
                "DBG usage parse: {} args in spec, {} pos",
                spec.args.len(),
                spec.positionals.len()
            );
        }

        let dbg = std::env::var_os("BRUSH_DBG").is_some();
        loop {
            match parser.next_event() {
                None => break,
                Some(Err(err)) => {
                    return Err(render_parse_error(&file_spec, &argv_refs, &err));
                }
                Some(Ok(Event::Flag { flag, value, .. })) => {
                    if dbg {
                        std::eprintln!("DBG flag event: {flag:?} value={value:?}");
                    }
                    let id = flag.name;
                    match value {
                        None => values.set_flag(id),
                        Some(bytes) => {
                            let value = usage::argv::as_str(bytes)
                                .map_err(|err| BuiltinArgParseError {
                                    message: format!("invalid UTF-8 for `{id}`: {err}"),
                                    help_request: false,
                                })?
                                .to_owned();
                            values.push_value(id, value);
                        }
                    }
                }
                Some(Ok(Event::Arg { arg, value, .. })) => {
                    if dbg {
                        std::eprintln!("DBG arg event: name={} value={value:?}", arg.name);
                    }
                    // N.B. Positional events carry the positional's id; their
                    // values live in the positional slots.
                    let value = usage::argv::as_str(value)
                        .map_err(|err| {
                            let id = arg.name;
                            BuiltinArgParseError {
                                message: format!("invalid UTF-8 for `{id}`: {err}"),
                                help_request: false,
                            }
                        })?
                        .to_owned();
                    values.push_positional_by_id(arg.name, value);
                }
                Some(Ok(Event::Command(_))) => {}
                Some(Ok(Event::External { .. })) => {}
            }
        }

        Ok(values)
    }

    fn detailed_help(
        &self,
        spec: &'static CommandSpec,
        name: &str,
    ) -> Result<String, crate::error::Error> {
        let command = build_command(spec, name);
        let help_flag = help_trigger(command);
        let file_spec = borrowed_spec(spec, Box::leak(name.to_owned().into_boxed_str()));
        let argv: [&OsStr; 1] = [OsStr::new(help_flag)];

        let mut parser = Parser::new(command, &argv);
        match parser.next_event() {
            Some(Err(UsageError::Help { cmd, long })) => Ok(usage::argv::help::render_styled(
                &file_spec,
                cmd,
                long,
                usage::argv::help::Style::PLAIN,
            )
            .unwrap_or_default()),
            _ => Err(
                crate::error::ErrorKind::Unimplemented("failed to trigger help rendering").into(),
            ),
        }
    }
}

/// Builds the engine's command graph from the neutral spec.
///
/// The engine requires `&'static Command<'static>`; the built graph is leaked
/// (bounded by the number of distinct builtin invocations).
#[must_use]
#[expect(clippy::too_many_lines, reason = "explicit engine structure")]
pub fn build_command(spec: &'static CommandSpec, name: &str) -> &'static Command<'static> {
    let mut flags: Vec<Flag<'static>> = Vec::new();
    for (ix, arg) in spec.args.iter().enumerate() {
        let shorts: Vec<u8> = arg.shorts.iter().map(|c| *c as u8).collect();
        let longs: Vec<&'static str> = arg.longs.to_vec();
        let key = ix as u64 + 1;

        let flag = match arg.kind {
            ArgKind::Flag => Flag {
                key,
                binding_key: key,
                binding_type: None,
                name: arg.id,
                longs: &longs.leak()[..],
                shorts: &shorts.leak()[..],
                negate: None,
                takes_value: false,
                variadic: false,
                var_max: None,
                delimiter: None,
                allow_hyphen_values: false,
                allow_negative_numbers: false,
                value_terminator: None,
                require_equals: false,
                value_optional: false,
                bool_value: true,
                default_missing: None,
                global: false,
                action: ArgAction::Set,
            },
            ArgKind::Value => Flag {
                key,
                binding_key: key,
                binding_type: None,
                name: arg.id,
                longs: &longs.leak()[..],
                shorts: &shorts.leak()[..],
                negate: None,
                takes_value: true,
                variadic: false,
                var_max: None,
                delimiter: None,
                allow_hyphen_values: false,
                allow_negative_numbers: false,
                value_terminator: None,
                require_equals: false,
                value_optional: false,
                bool_value: false,
                default_missing: None,
                global: false,
                action: ArgAction::Set,
            },
        };
        flags.push(flag);
    }

    let args: Vec<Arg<'static>> = spec
        .positionals
        .iter()
        .enumerate()
        .map(|(ix, p)| Arg {
            key: 1000 + ix as u64,
            required: !p.many,
            var: p.many,
            var_max: None,
            delimiter: None,
            allow_negative_numbers: false,
            value_terminator: None,
            double_dash: DoubleDash::Optional,
            name: p.id,
        })
        .collect();

    fn leak_flag(flag: Flag<'static>) -> &'static Flag<'static> {
        Box::leak(Box::new(flag))
    }
    fn leak_arg(arg: Arg<'static>) -> &'static Arg<'static> {
        Box::leak(Box::new(arg))
    }

    let flag_refs: &'static [&'static Flag<'static>] = Box::leak(
        flags
            .into_iter()
            .map(leak_flag)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let arg_refs: &'static [&'static Arg<'static>] = Box::leak(
        args.into_iter()
            .map(leak_arg)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );

    Box::leak(Box::new(Command {
        name: leak_str(name),
        aliases: &[],
        flags: flag_refs,
        args: arg_refs,
        subcommands: &[],
        default_subcommand: None,
        external_subcommand: false,
        arg_required_else_help: false,
        subcommand_negates_reqs: false,
        args_conflicts_with_subcommands: false,
        subcommand_precedence_over_arg: false,
        allow_missing_positional: false,
        dont_delimit_trailing_values: false,
        unknown_flags: None,
        version: false,
        disable_help_flag: false,
        disable_help_subcommand: true,
        disable_version_flag: true,
        key: 0,
    }))
}

fn help_trigger(command: &Command<'_>) -> &'static str {
    let has_short_help = command
        .flags
        .iter()
        .any(|f| f.shorts.contains(&(b'h')) && f.action == ArgAction::Help);
    if has_short_help { "-h" } else { "--help" }
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

/// Builds the borrowed help-time spec view.
#[must_use]
pub fn borrowed_spec(
    spec: &'static CommandSpec,
    name: &'static str,
) -> usage::argv::spec::Spec<'static> {
    let cmd = build_command(spec, name);
    usage::argv::spec::Spec {
        name: leak_str(name),
        bin: Some(leak_str(name)),
        root: Box::leak(Box::new(usage::argv::spec::CommandMeta {
            cmd,
            ..usage::argv::spec::CommandMeta::EMPTY
        })),
        ..usage::argv::spec::Spec::EMPTY
    }
}
