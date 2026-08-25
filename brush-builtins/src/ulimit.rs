//! The `ulimit` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ULimitCommand);

use ::clap::builder::{IntoResettable, StyledStr};
use brush_core::ExecutionResult;
use std::{
    io::{self, ErrorKind, Write},
    str::FromStr,
};

#[derive(Clone, Copy)]
pub(super) enum Unit {
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
pub(super) enum Virtual {
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
pub(super) enum Resource {
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
pub(super) struct ResourceDescription {
    resource: Resource,
    help: &'static str,
    description: &'static str,
    short: char,
    unit: Unit,
}

impl ResourceDescription {
    const SBSIZE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::SBSIZE),
        help: "the socket buffer size",
        description: "socket buffer size",
        short: 'b',
        unit: Unit::Bytes,
    };
    const CORE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::CORE),
        help: "the maximum size of core files created",
        description: "core file size",
        short: 'c',
        unit: Unit::Block,
    };
    const DATA: Self = Self {
        resource: Resource::Phy(rlimit::Resource::DATA),
        help: "the maximum size of a process's data segment",
        description: "data seg size",
        short: 'd',
        unit: Unit::KBytes,
    };
    const NICE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NICE),
        help: "the maximum scheduling priority (`nice`)",
        description: "scheduling priority",
        short: 'e',
        unit: Unit::Number,
    };
    const FSIZE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::FSIZE),
        help: "the maximum size of files written by the shell and its children",
        description: "file size",
        short: 'f',
        unit: Unit::Block,
    };
    const SIGPENDING: Self = Self {
        resource: Resource::Phy(rlimit::Resource::SIGPENDING),
        help: "the maximum number of pending signals",
        description: "pending signals",
        short: 'i',
        unit: Unit::Number,
    };
    const MEMLOCK: Self = Self {
        resource: Resource::Phy(rlimit::Resource::MEMLOCK),
        help: "the maximum size a process may lock into memory",
        description: "max locked memory",
        short: 'l',
        unit: Unit::KBytes,
    };
    const KQUEUES: Self = Self {
        resource: Resource::Phy(rlimit::Resource::KQUEUES),
        help: "the maximum number of kqueues allocated for this process",
        description: "max kqueues",
        short: 'k',
        unit: Unit::Number,
    };
    const RSS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::RSS),
        help: "the maximum resident set size",
        description: "max memory size",
        short: 'm',
        unit: Unit::KBytes,
    };
    const LOCKS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::LOCKS),
        help: "the maximum number of file locks",
        description: "file locks",
        short: 'x',
        unit: Unit::Number,
    };
    const NOFILE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NOFILE),
        help: "the maximum number of open file descriptors",
        description: "open files",
        short: 'n',
        unit: Unit::Number,
    };
    const MSGQUEUE: Self = Self {
        resource: Resource::Phy(rlimit::Resource::MSGQUEUE),
        help: "the maximum number of bytes in POSIX message queues",
        description: "POSIX message queues",
        short: 'q',
        unit: Unit::Bytes,
    };
    const PIPE: Self = Self {
        resource: Resource::Virt(Virtual::Pipe),
        help: "the pipe buffer size",
        description: "pipe size",
        short: 'p',
        unit: Unit::HalfKBytes,
    };
    const RTPRIO: Self = Self {
        resource: Resource::Phy(rlimit::Resource::RTPRIO),
        help: "the maximum real-time scheduling priority",
        description: "real-time priority",
        short: 'r',
        unit: Unit::Number,
    };
    const RTTIME: Self = Self {
        resource: Resource::Phy(rlimit::Resource::RTTIME),
        help: "the maximum real-time scheduling priority",
        description: "real-time non-blocking time",
        short: 'R',
        unit: Unit::Micros,
    };
    const STACK: Self = Self {
        resource: Resource::Phy(rlimit::Resource::STACK),
        help: "the maximum stack size",
        description: "stack size",
        short: 's',
        unit: Unit::KBytes,
    };
    const CPU: Self = Self {
        resource: Resource::Phy(rlimit::Resource::CPU),
        help: "the maximum amount of cpu time in seconds",
        description: "cpu time",
        short: 't',
        unit: Unit::Seconds,
    };
    const NPROC: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NPROC),
        help: "the maximum number of user processes",
        description: "max user processes",
        short: 'u',
        unit: Unit::Number,
    };
    const VMEM: Self = Self {
        resource: Resource::Virt(Virtual::VMem),
        help: "the size of virtual memory",
        description: "virtual memory",
        short: 'v',
        unit: Unit::KBytes,
    };
    const THREADS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::THREADS),
        help: "the maximum number of threads",
        description: "number of threads",
        short: 'T',
        unit: Unit::Number,
    };
    const NPTS: Self = Self {
        resource: Resource::Phy(rlimit::Resource::NPTS),
        help: "the maximum number of pseudoterminals",
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

    /// Provide the matching help String
    fn help(&self) -> String {
        format!(
            "{} {}",
            self.help,
            if self.resource.is_supported() {
                "(supported)"
            } else {
                "(unsupported)"
            }
        )
    }
}

impl IntoResettable<StyledStr> for ResourceDescription {
    fn into_resettable(self) -> ::clap::builder::Resettable<StyledStr> {
        ::clap::builder::Resettable::Value(self.help().into())
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum LimitValue {
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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ULimitCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let exit_code = ExecutionResult::success();
    let mut resources_to_set = Vec::new();
    let mut resources_to_get = Vec::new();

    let mut set_or_get = |val, descr| {
        match val {
            Some(LimitValue::Unset) => resources_to_get.push(descr),
            Some(v) => resources_to_set.push((descr, v)),
            None => {}
        }
        if command.all {
            resources_to_get.push(descr);
        }
    };

    set_or_get(command.sbsize, ResourceDescription::SBSIZE);
    set_or_get(command.core, ResourceDescription::CORE);
    set_or_get(command.data, ResourceDescription::DATA);
    set_or_get(command.file_size, ResourceDescription::FSIZE);
    set_or_get(command.sigpending, ResourceDescription::SIGPENDING);
    set_or_get(command.kqueues, ResourceDescription::KQUEUES);
    set_or_get(command.memlock, ResourceDescription::MEMLOCK);
    set_or_get(command.rss, ResourceDescription::RSS);
    set_or_get(command.file_lock, ResourceDescription::LOCKS);
    set_or_get(command.file_open, ResourceDescription::NOFILE);
    set_or_get(command.pipe, ResourceDescription::PIPE);
    set_or_get(command.npts, ResourceDescription::NPTS);
    set_or_get(command.nice, ResourceDescription::NICE);
    set_or_get(command.msgqueue, ResourceDescription::MSGQUEUE);
    set_or_get(command.rtprio, ResourceDescription::RTPRIO);
    set_or_get(command.rttime, ResourceDescription::RTTIME);
    set_or_get(command.stack, ResourceDescription::STACK);
    set_or_get(command.threads, ResourceDescription::THREADS);
    set_or_get(command.cpu, ResourceDescription::CPU);
    set_or_get(command.nproc, ResourceDescription::NPROC);
    set_or_get(command.vmem, ResourceDescription::VMEM);

    if resources_to_set.is_empty() {
        if resources_to_get.is_empty() {
            if let Some(fsize) = command.limit {
                resources_to_set.push((ResourceDescription::FSIZE, fsize));
            } else {
                resources_to_get.push(ResourceDescription::FSIZE);
            }
        }
    }

    for (resource, value) in resources_to_set {
        resource.set(command.hard, value)?;
    }

    if resources_to_get.len() == 1 {
        writeln!(
            context.stdout(),
            "{}",
            resources_to_get[0].get(command.hard)?
        )?;
    } else {
        for resource in resources_to_get {
            resource.print(&context, command.hard)?;
        }
    }

    Ok(exit_code)
}
