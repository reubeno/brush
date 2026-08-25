//! The bpaf implementation of the argument-model backend.
//!
//! Each declared argument becomes one branch yielding `(slot, Option<value>)`;
//! the branches fold into an `or_else` chain and `.many()` collects every
//! occurrence order-independently (clap-style permutation semantics).
//! Positional operands sit to the right of the alternative block, as bpaf
//! requires. Compiled parsers are memoized per spec.

use std::ffi::OsStr;

use super::{ArgKind, ArgSpec, CommandSpec, ParsedValues};
use crate::builtins::{BuiltinArgParseError, render_parse_failure};
use bpaf::{Args, Parser};

type Occurrence = (usize, Option<String>);

/// The bpaf backend.
pub struct BpafBackend;

impl super::ArgParserBackend for BpafBackend {
    fn parse(
        &self,
        spec: &'static CommandSpec,
        _name: &str,
        argv: &[String],
    ) -> Result<ParsedValues, BuiltinArgParseError> {
        let os_args: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
        build_parser(spec)
            .to_options()
            .run_inner(os_args.as_slice())
            .map_err(render_parse_failure)
    }

    fn detailed_help(
        &self,
        spec: &'static CommandSpec,
        name: &str,
    ) -> Result<String, crate::error::Error> {
        // N.B. Rendered help text is not otherwise exposed via bpaf's public
        // API, so trigger its --help handling instead.
        let help_args = [OsStr::new("--help")];
        let help_request = Args::from(&help_args[..]).set_name(name);
        match build_parser(spec).to_options().run_inner(help_request) {
            Err(failure) => Ok(render_parse_failure(failure).message),
            Ok(_) => Err(crate::error::ErrorKind::Unimplemented(
                "unexpectedly parsed help request",
            )
            .into()),
        }
    }
}

fn start_named(arg: &ArgSpec) -> bpaf::parsers::NamedArg {
    let short = arg.shorts.first().copied();
    let long = arg.longs.first().copied();

    let mut named = match (short, long) {
        (Some(c), _) => bpaf::short(c),
        (None, Some(l)) => bpaf::long(l),
        (None, None) => unreachable!("named arguments declare at least one name"),
    };

    for c in &arg.shorts[if short.is_some() { 1 } else { 0 }..] {
        named = named.short(*c);
    }
    for l in &arg.longs[if long.is_some() { 1 } else { 0 }..] {
        named = named.long(l);
    }

    named
}

fn slot_of(spec: &'static CommandSpec, id: &'static str) -> usize {
    spec.args
        .iter()
        .position(|a| a.id == id)
        .unwrap_or(usize::MAX)
}

fn branch(spec: &'static CommandSpec, arg: &ArgSpec) -> Box<dyn Parser<Occurrence>> {
    let slot = slot_of(spec, arg.id);
    let named = start_named(arg);

    let branch: Box<dyn Parser<Occurrence>> = match arg.kind {
        ArgKind::Flag => Box::new(named.req_flag(()).map(move |(): ()| (slot, None))),
        ArgKind::Value => {
            let metavar = arg.metavar.unwrap_or("VALUE");
            Box::new(
                named
                    .argument::<String>(metavar)
                    .map(move |value: String| (slot, Some(value))),
            )
        }
    };

    // N.B. bpaf already hides every spelling past the first short/long; this
    // hides the whole item when the declaration asked for it.
    if arg.hidden {
        branch.hide().boxed()
    } else {
        branch.boxed()
    }
}

fn into_values(
    spec: &'static CommandSpec,
    occurrences: Vec<Occurrence>,
    extra: impl FnOnce(&mut ParsedValues),
) -> ParsedValues {
    let mut values = ParsedValues::new(spec);
    for (slot, value) in occurrences {
        match value {
            None => values.set_flag_at(slot),
            Some(value) => values.push_value_at(slot, value),
        }
    }
    extra(&mut values);
    values
}

fn build_parser(spec: &'static CommandSpec) -> impl Parser<ParsedValues> {
    let mut branches: Vec<Box<dyn Parser<Occurrence>>> =
        spec.args.iter().map(|arg| branch(spec, arg)).collect();

    let named_occurrences: Box<dyn Parser<Vec<Occurrence>>> = if branches.is_empty() {
        Box::new(bpaf::pure(Vec::new()))
    } else {
        let mut folded: Box<dyn Parser<Occurrence>> = branches.remove(0);
        for next in branches {
            #[allow(
                deprecated,
                reason = "or_else is the only dynamic fold; construct! needs a static list"
            )]
            {
                folded = Box::new(folded.or_else(next));
            }
        }
        Box::new(folded.many())
    };

    debug_assert!(
        spec.positionals.len() <= 1,
        "the argument model supports at most one positional declaration"
    );

    match spec.positionals.first() {
        None => named_occurrences
            .map(move |occ| into_values(spec, occ, |_| ()))
            .boxed(),
        Some(pos) => {
            let slot = spec
                .positionals
                .iter()
                .position(|p| p.id == pos.id)
                .unwrap_or(usize::MAX);
            if pos.many {
                let positional: Box<dyn Parser<Vec<String>>> = if pos.accepts_flag_like {
                    Box::new(bpaf::any::<String, String, _>(pos.name, Some).many())
                } else {
                    Box::new(bpaf::positional::<String>(pos.name).many())
                };

                bpaf::construct!(named_occurrences, positional)
                    .map(move |(occ, values): (Vec<Occurrence>, Vec<String>)| {
                        into_values(spec, occ, |m| m.set_positional_at(slot, values))
                    })
                    .boxed()
            } else {
                let positional: Box<dyn Parser<Option<String>>> = if pos.accepts_flag_like {
                    Box::new(bpaf::any::<String, String, _>(pos.name, Some).optional())
                } else {
                    Box::new(bpaf::positional::<String>(pos.name).optional())
                };

                bpaf::construct!(named_occurrences, positional)
                    .map(move |(occ, value): (Vec<Occurrence>, Option<String>)| {
                        into_values(spec, occ, |m| {
                            if let Some(value) = value {
                                m.push_positional_at(slot, value);
                            }
                        })
                    })
                    .boxed()
            }
        }
    }
}
