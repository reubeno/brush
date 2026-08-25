use std::{
    collections::HashMap,
    io::{self, ErrorKind, Write},
    str::FromStr,
};

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues, PositionalSpec};
use brush_core::{ExecutionResult, builtins};

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

impl Unit {
    const fn scale(self) -> u64 {
        match self {
            Self::Block | Self::HalfKBytes => 512,
            Self::KBytes => 1024,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy)]
enum Virtual {
    Pipe,
    VMem,
}

impl Virtual {
    fn get(self) -> std::io::Result<(u64, u64)> {
        match self {
            Self::Pipe => {
                let lim = nix::unistd::PathconfVar::PIPE_BUF as u64 * 512;
                Ok((lim, lim))
            }
            Self::VMem => rlimit::Resource::AS
                .get()
                .or_else(|_| rlimit::Resource::VMEM.get()),
        }
    }
    fn set(self, soft: u64, hard: u64) -> std::io::Result<()> {
        match self {
            Self::Pipe => Err(std::io::Error::from(ErrorKind::Unsupported)),
            Self::VMem => rlimit::Resource::AS
                .set(soft, hard)
                .or_else(|_| rlimit::Resource::VMEM.set(soft, hard)),
        }
    }
    const fn is_supported(self) -> bool {
        match self {
            Self::Pipe => true,
            Self::VMem => {
                rlimit::Resource::AS.is_supported() || rlimit::Resource::VMEM.is_supported()
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Resource {
    Phy(rlimit::Resource),
    Virt(Virtual),
}

impl Resource {
    fn get(self) -> std::io::Result<(u64, u64)> {
        match self {
            Self::Phy(res) => res.get(),
            Self::Virt(res) => res.get(),
        }
    }
    fn set(self, soft: u64, hard: u64) -> std::io::Result<()> {
        match self {
            Self::Phy(res) => res.set(soft, hard),
            Self::Virt(res) => res.set(soft, hard),
        }
    }
    const fn is_supported(self) -> bool {
        match self {
            Self::Phy(res) => res.is_supported(),
            Self::Virt(res) => res.is_supported(),
        }
    }
}

#[derive(Clone, Copy)]
struct ResourceDescription {
    resource: Resource,
    description: &'static str,
    short: char,
    unit: Unit,
}

impl ResourceDescription {
    const SBSIZE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::SBSIZE),
        description: "socket buffer size",
        short: 'b',
        unit: Unit::Bytes,
    };
    const CORE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::CORE),
        description: "core file size",
        short: 'c',
        unit: Unit::Block,
    };
    const DATA: Self = Self {
        resource: Resource::Phy(rlimit::Resource::DATA),
        description: "data seg size",
        short: 'd',
        unit: Unit::KBytes,
    };
    const NICE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NICE),
        description: "scheduling priority",
        short: 'e',
        unit: Unit::Number,
    };
    const FSIZE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::FSIZE),
        description: "file size",
        short: 'f',
        unit: Unit::Block,
    };
    const SIGPENDING: Self = Self {
        resource: Resource::Phy(rlimit::Resource::SIGPENDING),
        description: "pending signals",
        short: 'i',
        unit: Unit::Number,
    };
    const MEMLOCK: Self = Self {
        resource: Resource::Phy(rlimit::Resource::MEMLOCK),
        description: "max locked memory",
        short: 'l',
        unit: Unit::KBytes,
    };
    const KQUEUES: Self = Self {
        resource: Resource::Phy(rlimit::Resource::KQUEUES),
        description: "max kqueues",
        short: 'k',
        unit: Unit::Number,
    };
    const RSS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::RSS),
        description: "max memory size",
        short: 'm',
        unit: Unit::KBytes,
    };
    const LOCKS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::LOCKS),
        description: "file locks",
        short: 'x',
        unit: Unit::Number,
    };
    const NOFILE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NOFILE),
        description: "open files",
        short: 'n',
        unit: Unit::Number,
    };
    const MSGQUEUE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::MSGQUEUE),
        description: "POSIX message queues",
        short: 'q',
        unit: Unit::Bytes,
    };
    const PIPE: Self = Self {
        resource: Resource::Virt(Virtual::Pipe),
        description: "pipe size",
        short: 'p',
        unit: Unit::HalfKBytes,
    };
    const RTPRIO: Self = Self {
        resource: Resource::Phy(rlimit::Resource::RTPRIO),
        description: "real-time priority",
        short: 'r',
        unit: Unit::Number,
    };
    const RTTIME: Self = Self {
        resource: Resource::Phy(rlimit::Resource::RTTIME),
        description: "real-time non-blocking time",
        short: 'R',
        unit: Unit::Micros,
    };
    const STACK: Self = Self {
        resource: Resource::Phy(rlimit::Resource::STACK),
        description: "stack size",
        short: 's',
        unit: Unit::KBytes,
    };
    const CPU: Self = Self {
        resource: Resource::Phy(rlimit::Resource::CPU),
        description: "cpu time",
        short: 't',
        unit: Unit::Seconds,
    };
    const NPROC: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NPROC),
        description: "max user processes",
        short: 'u',
        unit: Unit::Number,
    };
    const VMEM: Self = Self {
        resource: Resource::Virt(Virtual::VMem),
        description: "virtual memory",
        short: 'v',
        unit: Unit::KBytes,
    };
    const THREADS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::THREADS),
        description: "number of threads",
        short: 'T',
        unit: Unit::Number,
    };
    const NPTS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NPTS),
        description: "number of pseudoterminals",
        short: 'P',
        unit: Unit::Number,
    };

    fn get(&self, hard: bool) -> std::io::Result<String> {
        let (soft_limit, hard_limit) = self.resource.get()?;
        let val = if hard { hard_limit } else { soft_limit };

        if val == rlimit::INFINITY {
            Ok("unlimited".into())
        } else {
            Ok(format!("{}", val / self.unit.scale()))
        }
    }

    fn set(&self, set_hard: bool, value: LimitValue) -> std::io::Result<()> {
        let (soft, hard) = self.resource.get()?;
        let value = match value {
            LimitValue::Soft => soft,
            LimitValue::Hard => hard,
            LimitValue::Unlimited => rlimit::INFINITY,
            LimitValue::Value(v) => v * self.unit.scale(),
            LimitValue::Unset => return Ok(()),
        };

        if set_hard {
            self.resource.set(soft, value)
        } else {
            self.resource.set(value, hard)
        }
    }

    /// Print either soft or hard limit
    fn print(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        hard: bool,
    ) -> io::Result<()> {
        if !self.resource.is_supported() {
            return Ok(());
        }
        let unit = match self.unit {
            Unit::Block => format!("(block, -{})", self.short),
            Unit::Bytes => format!("(bytes, -{})", self.short),
            Unit::HalfKBytes => format!("(512 bytes, -{})", self.short),
            Unit::KBytes => format!("(kbytes, -{})", self.short),
            Unit::Micros => format!("(microseconds, -{})", self.short),
            Unit::Number => format!("(-{})", self.short),
            Unit::Seconds => format!("(seconds, -{})", self.short),
        };
        let resource = self.get(hard).unwrap_or_else(|e| format!("{e}"));
        writeln!(
            context.stdout(),
            "{:<26}{:>16} {}",
            self.description,
            unit,
            resource
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum LimitValue {
    Unset,
    Unlimited,
    Soft,
    Hard,
    Value(u64),
}

impl FromStr for LimitValue {
    type Err = <u64 as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = match s {
            "" => Self::Unset,
            "unlimited" => Self::Unlimited,
            "soft" => Self::Soft,
            "hard" => Self::Hard,
            _ => Self::Value(s.parse()?),
        };
        Ok(v)
    }
}

/// Removes bare resource-limit switches (e.g., `-c` with no value, meaning
/// "report this limit") from the token stream, recording them as
/// [`LimitValue::Unset`]. Value-taking forms (`-c=5`, `-c 5`) are left in the
/// stream for the argument backend to bind.
///
/// This mirrors the historical parser's dual switch/value semantics, which
/// the backend's required-value options cannot express on their own.
fn extract_report_switches(
    args: Vec<String>,
    limits: &mut HashMap<char, LimitValue>,
) -> Vec<String> {
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        match arg.strip_prefix('-').and_then(|body| {
            let mut chars = body.chars();
            let c = chars.next()?;
            (chars.next().is_none() && VALUE_SHORTS.contains(&c)).then_some(c)
        }) {
            // N.B. When a plausible value word follows, treat the option as
            // value-taking and defer to the backend; otherwise the switch is
            // a request to report the limit.
            Some(c) => match iter.peek() {
                Some(next) if !next.starts_with('-') && next.parse::<LimitValue>().is_ok() => {
                    rest.push(arg);
                }
                _ => {
                    limits.insert(c, LimitValue::Unset);
                }
            },
            None => rest.push(arg),
        }
    }

    rest
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

/// Parses an optional resource-limit value that the backend bound to the
/// option with the given declaration id.
fn parse_resource_value(
    values: &ParsedValues,
    id: &str,
) -> Result<Option<LimitValue>, builtins::BuiltinArgParseError> {
    match values.value(id) {
        Some(s) => LimitValue::from_str(s)
            .map(Some)
            .map_err(|_| builtins::BuiltinArgParseError {
                message: format!("invalid value for limit: {s}"),
                help_request: false,
            }),
        None => Ok(None),
    }
}

/// Modify shell resource limits.
///
/// Provides control over the resources available to the shell and processes
/// it creates, on systems that allow such control.
pub(crate) struct ULimitCommand {
    #[expect(dead_code)]
    soft: bool,
    hard: bool,
    all: bool,
    sbsize: Option<LimitValue>,
    core: Option<LimitValue>,
    data: Option<LimitValue>,
    nice: Option<LimitValue>,
    file_size: Option<LimitValue>,
    sigpending: Option<LimitValue>,
    memlock: Option<LimitValue>,
    kqueues: Option<LimitValue>,
    rss: Option<LimitValue>,
    file_open: Option<LimitValue>,
    pipe: Option<LimitValue>,
    msgqueue: Option<LimitValue>,
    rtprio: Option<LimitValue>,
    rttime: Option<LimitValue>,
    stack: Option<LimitValue>,
    cpu: Option<LimitValue>,
    nproc: Option<LimitValue>,
    vmem: Option<LimitValue>,
    file_lock: Option<LimitValue>,
    npts: Option<LimitValue>,
    threads: Option<LimitValue>,
    limit: Option<LimitValue>,
}

static ULIMIT_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag("soft", &['S'], &[], "Use the `soft` resource limit."),
        ArgSpec::flag("hard", &['H'], &[], "Use the `hard` resource limit."),
        ArgSpec::flag("all", &['a'], &[], "All current limits are reported."),
        // N.B. Value-taking forms are bound by the backend parser; bare
        // occurrences never reach it (see `extract_report_switches`).
        ArgSpec::value(
            "sbsize",
            &['b'],
            &[],
            "LIMIT",
            "The maximum socket buffer size.",
        ),
        ArgSpec::value(
            "core",
            &['c'],
            &[],
            "LIMIT",
            "The maximum size of core files created.",
        ),
        ArgSpec::value(
            "data",
            &['d'],
            &[],
            "LIMIT",
            "The maximum size of a process's data segment.",
        ),
        ArgSpec::value(
            "nice",
            &['e'],
            &[],
            "LIMIT",
            "The maximum scheduling priority (`nice`).",
        ),
        ArgSpec::value(
            "file_size",
            &['f'],
            &[],
            "LIMIT",
            "The maximum size of files written by the shell and its children.",
        ),
        ArgSpec::value(
            "sigpending",
            &['i'],
            &[],
            "LIMIT",
            "The maximum number of pending signals.",
        ),
        ArgSpec::value(
            "memlock",
            &['l'],
            &[],
            "LIMIT",
            "The maximum size a process may lock into memory.",
        ),
        ArgSpec::value(
            "kqueues",
            &['k'],
            &[],
            "LIMIT",
            "The maximum number of kqueues allocated for this process.",
        ),
        ArgSpec::value(
            "rss",
            &['m'],
            &[],
            "LIMIT",
            "The maximum resident set size.",
        ),
        ArgSpec::value(
            "file_open",
            &['n'],
            &[],
            "LIMIT",
            "The maximum number of open file descriptors.",
        ),
        ArgSpec::value("pipe", &['p'], &[], "LIMIT", "The pipe buffer size."),
        ArgSpec::value(
            "msgqueue",
            &['q'],
            &[],
            "LIMIT",
            "The maximum number of bytes in POSIX message queues.",
        ),
        ArgSpec::value(
            "rtprio",
            &['r'],
            &[],
            "LIMIT",
            "The maximum real-time scheduling priority.",
        ),
        ArgSpec::value(
            "rttime",
            &['R'],
            &[],
            "LIMIT",
            "Real-time non-blocking time.",
        ),
        ArgSpec::value("stack", &['s'], &[], "LIMIT", "The maximum stack size."),
        ArgSpec::value(
            "cpu",
            &['t'],
            &[],
            "LIMIT",
            "The maximum amount of cpu time in seconds.",
        ),
        ArgSpec::value(
            "nproc",
            &['u'],
            &[],
            "LIMIT",
            "The maximum number of user processes.",
        ),
        ArgSpec::value("vmem", &['v'], &[], "LIMIT", "The size of virtual memory."),
        ArgSpec::value(
            "file_lock",
            &['x'],
            &[],
            "LIMIT",
            "The maximum number of file locks.",
        ),
        ArgSpec::value(
            "npts",
            &['P'],
            &[],
            "LIMIT",
            "The maximum number of pseudoterminals.",
        ),
        ArgSpec::value(
            "threads",
            &['T'],
            &[],
            "LIMIT",
            "The maximum number of threads.",
        ),
    ],
    positionals: &[PositionalSpec::one("limit", "LIMIT")],
};

impl builtins::SpecCommand for ULimitCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        &ULIMIT_SPEC
    }

    /// Overrides the default [`builtins::SpecCommand::new`] flow to split
    /// attached values out of grouped resource options first and to capture
    /// bare value-less switches; see [`expand_limit_option_groups`] and
    /// [`extract_report_switches`].
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        let expanded = expand_limit_option_groups(args);

        let mut report_switches = HashMap::new();
        let remaining = extract_report_switches(expanded, &mut report_switches);

        let mut values =
            brush_core::builtins::argmodel::backend().parse(Self::spec(), "", &remaining)?;

        let mut command = Self::from_matches(&mut values)?;

        if !report_switches.is_empty() {
            command.apply_report_switches(&report_switches);
        }

        Ok(command)
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        let limit = match values.value("limit") {
            Some(s) => {
                Some(
                    LimitValue::from_str(s).map_err(|_| builtins::BuiltinArgParseError {
                        message: format!("invalid limit value: {s}"),
                        help_request: false,
                    })?,
                )
            }
            None => None,
        };

        Ok(Self {
            soft: values.flag("soft"),
            hard: values.flag("hard"),
            all: values.flag("all"),
            sbsize: parse_resource_value(values, "sbsize")?,
            core: parse_resource_value(values, "core")?,
            data: parse_resource_value(values, "data")?,
            nice: parse_resource_value(values, "nice")?,
            file_size: parse_resource_value(values, "file_size")?,
            sigpending: parse_resource_value(values, "sigpending")?,
            memlock: parse_resource_value(values, "memlock")?,
            kqueues: parse_resource_value(values, "kqueues")?,
            rss: parse_resource_value(values, "rss")?,
            file_open: parse_resource_value(values, "file_open")?,
            pipe: parse_resource_value(values, "pipe")?,
            msgqueue: parse_resource_value(values, "msgqueue")?,
            rtprio: parse_resource_value(values, "rtprio")?,
            rttime: parse_resource_value(values, "rttime")?,
            stack: parse_resource_value(values, "stack")?,
            cpu: parse_resource_value(values, "cpu")?,
            nproc: parse_resource_value(values, "nproc")?,
            vmem: parse_resource_value(values, "vmem")?,
            file_lock: parse_resource_value(values, "file_lock")?,
            npts: parse_resource_value(values, "npts")?,
            threads: parse_resource_value(values, "threads")?,
            limit,
        })
    }

    fn about() -> &'static str {
        "Modify shell resource limits."
    }

    fn synopsis() -> &'static str {
        "[-SHa] [LIMIT]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let exit_code = ExecutionResult::success();
        let mut resources_to_set = Vec::new();
        let mut resources_to_get = Vec::new();

        let mut set_or_get = |val, descr| {
            match val {
                Some(LimitValue::Unset) => resources_to_get.push(descr),
                Some(v) => resources_to_set.push((descr, v)),
                None => {}
            }
            if self.all {
                resources_to_get.push(descr);
            }
        };

        set_or_get(self.sbsize, ResourceDescription::SBSIZE);
        set_or_get(self.core, ResourceDescription::CORE);
        set_or_get(self.data, ResourceDescription::DATA);
        set_or_get(self.file_size, ResourceDescription::FSIZE);
        set_or_get(self.sigpending, ResourceDescription::SIGPENDING);
        set_or_get(self.kqueues, ResourceDescription::KQUEUES);
        set_or_get(self.memlock, ResourceDescription::MEMLOCK);
        set_or_get(self.rss, ResourceDescription::RSS);
        set_or_get(self.file_lock, ResourceDescription::LOCKS);
        set_or_get(self.file_open, ResourceDescription::NOFILE);
        set_or_get(self.pipe, ResourceDescription::PIPE);
        set_or_get(self.npts, ResourceDescription::NPTS);
        set_or_get(self.nice, ResourceDescription::NICE);
        set_or_get(self.msgqueue, ResourceDescription::MSGQUEUE);
        set_or_get(self.rtprio, ResourceDescription::RTPRIO);
        set_or_get(self.rttime, ResourceDescription::RTTIME);
        set_or_get(self.stack, ResourceDescription::STACK);
        set_or_get(self.threads, ResourceDescription::THREADS);
        set_or_get(self.cpu, ResourceDescription::CPU);
        set_or_get(self.nproc, ResourceDescription::NPROC);
        set_or_get(self.vmem, ResourceDescription::VMEM);

        if resources_to_set.is_empty() {
            if resources_to_get.is_empty() {
                if let Some(fsize) = self.limit {
                    resources_to_set.push((ResourceDescription::FSIZE, fsize));
                } else {
                    resources_to_get.push(ResourceDescription::FSIZE);
                }
            }
        }

        for (resource, value) in resources_to_set {
            resource.set(self.hard, value)?;
        }

        if resources_to_get.len() == 1 {
            writeln!(context.stdout(), "{}", resources_to_get[0].get(self.hard)?)?;
        } else {
            for resource in resources_to_get {
                resource.print(&context, self.hard)?;
            }
        }

        Ok(exit_code)
    }
}

impl ULimitCommand {
    /// Applies bare value-less switches captured by
    /// [`extract_report_switches`] onto the parsed command.
    fn apply_report_switches(&mut self, switches: &HashMap<char, LimitValue>) {
        self.sbsize = switches.get(&'b').copied();
        self.core = switches.get(&'c').copied();
        self.data = switches.get(&'d').copied();
        self.nice = switches.get(&'e').copied();
        self.file_size = switches.get(&'f').copied();
        self.sigpending = switches.get(&'i').copied();
        self.memlock = switches.get(&'l').copied();
        self.kqueues = switches.get(&'k').copied();
        self.rss = switches.get(&'m').copied();
        self.file_open = switches.get(&'n').copied();
        self.pipe = switches.get(&'p').copied();
        self.msgqueue = switches.get(&'q').copied();
        self.rtprio = switches.get(&'r').copied();
        self.rttime = switches.get(&'R').copied();
        self.stack = switches.get(&'s').copied();
        self.cpu = switches.get(&'t').copied();
        self.nproc = switches.get(&'u').copied();
        self.vmem = switches.get(&'v').copied();
        self.file_lock = switches.get(&'x').copied();
        self.npts = switches.get(&'P').copied();
        self.threads = switches.get(&'T').copied();
    }
}
