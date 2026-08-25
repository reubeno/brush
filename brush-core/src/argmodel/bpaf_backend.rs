//! The bpaf implementation of the argument-model backend.
//!
//! Each declared argument becomes one branch yielding `(id, Option<value>)`;
//! the branches fold into an `or_else` chain and `.many()` collects every
//! occurrence order-independently (clap-style permutation semantics).
//! Positional operands sit to the right of the alternative block, as bpaf
//! requires.

#![cfg(feature = "parser-bpaf")]

use std::ffi::OsStr;

use super::{ArgKind, ArgSpec, CommandSpec, Matches};
use crate::builtins::{BuiltinArgParseError, render_parse_failure};
use bpaf::Parser;

type Occurrence = (&'static str, Option<String>);

/// The bpaf backend.
pub struct BpafBackend;

impl super::ArgParserBackend for BpafBackend {
    fn parse(
        &self,
        spec: &CommandSpec,
        _name: &str,
        argv: &[String],
    ) -> Result<Matches, BuiltinArgParseError> {
        let os_args: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
        let parser = build_parser(spec);

        parser
            .to_options()
            .run_inner(os_args.as_slice())
            .map_err(render_parse_failure)
    }

    fn detailed_help(&self, spec: &CommandSpec, name: &str) -> Result<String, crate::error::Error> {
        // N.B. Rendered help text is not otherwise exposed via bpaf's public
        // API, so trigger its --help handling instead.
        let help_args = [OsStr::new("--help")];
        let help_request = bpaf::Args::from(&help_args[..]).set_name(name);
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

fn branch(arg: &ArgSpec) -> Box<dyn Parser<Occurrence>> {
    let id: &'static str = arg.id;
    let named = start_named(arg);

    let branch: Box<dyn Parser<Occurrence>> = match arg.kind {
        ArgKind::Flag => Box::new(named.req_flag(()).map(move |(): ()| (id, None))),
        ArgKind::Value => {
            let metavar = arg.metavar.unwrap_or("VALUE");
            Box::new(
                named
                    .argument::<String>(metavar)
                    .map(move |value: String| (id, Some(value))),
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

fn build_parser(spec: &CommandSpec) -> impl Parser<Matches> + use<> {
    let mut branches: Vec<Box<dyn Parser<Occurrence>>> = spec.args.iter().map(branch).collect();

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
            .map(|occ| into_matches(occ, |_| ()))
            .boxed(),
        Some(pos) => {
            let id = pos.id;
            // N.B. Shell operands frequently look like flags (`echo -x`);
            // declarations opt into accepting such words via
            // `accepts_flag_like`. Strict `positional` parsing rejects them.
            let many_and_flag_like = pos.many && pos.accepts_flag_like;
            let many_strict = pos.many && !pos.accepts_flag_like;

            if many_and_flag_like || many_strict {
                let positional: Box<dyn Parser<Vec<String>>> = if many_and_flag_like {
                    Box::new(bpaf::any::<String, String, _>(pos.name, Some).many())
                } else {
                    Box::new(bpaf::positional::<String>(pos.name).many())
                };

                bpaf::construct!(named_occurrences, positional)
                    .map(move |(occ, values): (Vec<Occurrence>, Vec<String>)| {
                        into_matches(occ, |m| m.set_values(id, values))
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
                        into_matches(occ, |m| {
                            if let Some(value) = value {
                                m.push_value(id, value);
                            }
                        })
                    })
                    .boxed()
            }
        }
    }
}

fn into_matches(occurrences: Vec<Occurrence>, extra: impl FnOnce(&mut Matches)) -> Matches {
    let mut matches = Matches::new();
    for (id, value) in occurrences {
        match value {
            None => matches.set_flag(id),
            Some(value) => matches.push_value(id, value),
        }
    }
    extra(&mut matches);
    matches
}
