//! `ulimit` builtin: `ULimitCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::ffi::OsStr;

use bpaf::Parser;
use super::LimitValue;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

#[derive(Clone, Copy)]
enum Unit {
    Block,
    Bytes,
    HalfKBytes,
    KBytes,
    Micros,
    Number,
    Seconds,
}

#[derive(Clone, Copy)]
enum Virtual {
    Pipe,
    VMem,
}

#[derive(Clone, Copy)]
enum Resource {
    Phy(rlimit::Resource),
    Virt(Virtual),
}

#[derive(Clone, Copy)]
struct ResourceDescription {
    resource: Resource,
    description: &'static str,
    short: char,
    unit: Unit,
}


/// Returns a parser for a resource-limit switch that may be specified either
/// with a value (`-c 5`) or without one (`-c`, meaning "report this limit").
fn limit_switch(short: char, desc: &'static str) -> impl bpaf::Parser<Option<LimitValue>> {
    let with_value = bpaf::short(short)
        .help(desc)
        .argument::<LimitValue>("LIMIT");
    let without_value = bpaf::short(short)
        .req_flag(())
        .map(|(): ()| LimitValue::Unset);

    bpaf::construct!([with_value, without_value]).optional()
}

const SWITCH_SHORTS: &[char] = &['S', 'H', 'a'];

const VALUE_SHORTS: &[char] = &[
    'b', 'c', 'd', 'e', 'f', 'i', 'k', 'l', 'm', 'n', 'p', 'q', 'r', 's', 't', 'u', 'v', 'x', 'P',
    'R', 'T',
];

/// Splits attached values off of resource-limit option groups (e.g., `-c5`
/// becomes `-c=5` and `-Sc` becomes `-S -c`) so that grouped forms parse;
/// bpaf cannot disambiguate shorts that are registered as both flags and
/// value-taking arguments.
fn expand_limit_option_groups(args: Vec<String>) -> Vec<String> {
    let mut expanded = Vec::with_capacity(args.len());

    for arg in args {
        let Some(group) = arg
            .strip_prefix('-')
            .filter(|group| !group.is_empty() && !group.starts_with('-') && !group.contains('='))
        else {
            expanded.push(arg);
            continue;
        };

        let Some((head, value_short, tail)) =
            split_group_at_value_short(group, &VALUE_SHORTS.iter().collect::<String>())
        else {
            expanded.push(arg);
            continue;
        };

        if !head.chars().all(|c| SWITCH_SHORTS.contains(&c)) {
            expanded.push(arg);
            continue;
        }

        if !head.is_empty() {
            expanded.push(format!("-{head}"));
        }

        if tail.is_empty() {
            expanded.push(format!("-{value_short}"));
        } else {
            expanded.push(format!("-{value_short}={tail}"));
        }
    }

    expanded
}

/// Splits the given short-option group at its first value-taking option
/// character, returning the leading switch characters, the value-taking
/// character itself, and any attached value.
fn split_group_at_value_short<'a>(
    group: &'a str,
    value_shorts: &str,
) -> Option<(&'a str, char, &'a str)> {
    for (head, rest) in group
        .char_indices()
        .map(|(ix, _)| group.split_at(ix))
        .skip(1)
    {
        let mut chars = rest.chars();
        let Some(c) = chars.next() else {
            continue;
        };

        if value_shorts.contains(c) {
            return Some((head, c, chars.as_str()));
        }
    }

    None
}

fn render_parse_failure(failure: bpaf::ParseFailure) -> ArgsError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => ArgsError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => ArgsError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => ArgsError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

/// Modify shell resource limits.
///
/// Provides control over the resources available to the shell and processes
/// it creates, on systems that allow such control.
pub(crate) struct ULimitCommand {
    #[allow(dead_code)]
    pub(super) soft: bool,
    pub(super) hard: bool,
    pub(super) all: bool,
    pub(super) sbsize: Option<LimitValue>,
    pub(super) core: Option<LimitValue>,
    pub(super) data: Option<LimitValue>,
    pub(super) nice: Option<LimitValue>,
    pub(super) file_size: Option<LimitValue>,
    pub(super) sigpending: Option<LimitValue>,
    pub(super) memlock: Option<LimitValue>,
    pub(super) kqueues: Option<LimitValue>,
    pub(super) rss: Option<LimitValue>,
    pub(super) file_open: Option<LimitValue>,
    pub(super) pipe: Option<LimitValue>,
    pub(super) msgqueue: Option<LimitValue>,
    pub(super) rtprio: Option<LimitValue>,
    pub(super) rttime: Option<LimitValue>,
    pub(super) stack: Option<LimitValue>,
    pub(super) cpu: Option<LimitValue>,
    pub(super) nproc: Option<LimitValue>,
    pub(super) vmem: Option<LimitValue>,
    pub(super) file_lock: Option<LimitValue>,
    pub(super) npts: Option<LimitValue>,
    pub(super) threads: Option<LimitValue>,
    pub(super) limit: Option<LimitValue>,
}

impl crate::args::bpaf_support::BpafArgs for ULimitCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let soft = bpaf::short('S')
            .help("Use the `soft` resource limit.")
            .switch();
        let hard = bpaf::short('H')
            .help("Use the `hard` resource limit.")
            .switch();
        let all = bpaf::short('a')
            .help("All current limits are reported.")
            .switch();

        let sbsize = limit_switch('b', "The maximum socket buffer size.");
        let core = limit_switch('c', "The maximum size of core files created.");
        let data = limit_switch('d', "The maximum size of a process's data segment.");
        let nice = limit_switch('e', "The maximum scheduling priority (`nice`).");
        let file_size = limit_switch(
            'f',
            "The maximum size of files written by the shell and its children.",
        );
        let sigpending = limit_switch('i', "The maximum number of pending signals.");
        let memlock = limit_switch('l', "The maximum size a process may lock into memory.");
        let kqueues = limit_switch(
            'k',
            "The maximum number of kqueues allocated for this process.",
        );
        let rss = limit_switch('m', "The maximum resident set size.");
        let file_open = limit_switch('n', "The maximum number of open file descriptors.");
        let pipe = limit_switch('p', "The pipe buffer size.");
        let msgqueue = limit_switch('q', "The maximum number of bytes in POSIX message queues.");
        let rtprio = limit_switch('r', "The maximum real-time scheduling priority.");
        let rttime = limit_switch('R', "Real-time non-blocking time.");
        let stack = limit_switch('s', "The maximum stack size.");
        let cpu = limit_switch('t', "The maximum amount of cpu time in seconds.");
        let nproc = limit_switch('u', "The maximum number of user processes.");
        let vmem = limit_switch('v', "The size of virtual memory.");
        let file_lock = limit_switch('x', "The maximum number of file locks.");
        let npts = limit_switch('P', "The maximum number of pseudoterminals.");
        let threads = limit_switch('T', "The maximum number of threads.");

        let limit = bpaf::positional::<LimitValue>("LIMIT")
            .help("Argument for the implicit limit (`-f`).")
            .optional();

        bpaf::construct!(ULimitCommand {
            soft,
            hard,
            all,
            sbsize,
            core,
            data,
            nice,
            file_size,
            sigpending,
            memlock,
            kqueues,
            rss,
            file_open,
            pipe,
            msgqueue,
            rtprio,
            rttime,
            stack,
            cpu,
            nproc,
            vmem,
            file_lock,
            npts,
            threads,
            limit,
        })
    }
fn about() -> &'static str {
        "Modify shell resource limits."
    }
fn synopsis() -> &'static str {
        "[-SHa] [LIMIT]"
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        let expanded = expand_limit_option_groups(args);
        let os_args: Vec<&OsStr> = expanded.iter().map(OsStr::new).collect();

        Self::parser()
            .to_options()
            .run_inner(os_args.as_slice())
            .map_err(render_parse_failure)
    
    }
}

impl FromArgs for ULimitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ULimitCommand {
    type Error = brush_core::Error;

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
