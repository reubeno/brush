use clap::{
    builder::{IntoResettable, StyledStr},
    Parser,
};
use std::{io::ErrorKind, str::FromStr};

use crate::{builtins, commands};

#[derive(Clone, Copy)]
enum Unit {
    Block,
    Bytes,
    HalfKBytes,
    KBytes,
    Number,
    Seconds,
}

#[derive(Clone, Copy)]
enum Virtual {
    Pipe,
}

impl Virtual {
    fn get(&self) -> std::io::Result<(u64, u64)> {
        match self {
            Virtual::Pipe => {
                let lim = nix::unistd::PathconfVar::PIPE_BUF as u64;
                Ok((lim, lim))
            }
        }
    }
    fn set(&self, _soft: u64, _hard: u64) -> std::io::Result<()> {
        match self {
            Virtual::Pipe => Err(std::io::Error::from(ErrorKind::Unsupported)),
        }
    }
    fn is_supported(&self) -> bool {
        let _ = self;
        true
    }
}

#[derive(Clone, Copy)]
enum Resource {
    Phy(rlimit::Resource),
    Virt(Virtual),
}

impl Resource {
    fn get(&self) -> std::io::Result<(u64, u64)> {
        match self {
            Resource::Phy(res) => res.get(),
            Resource::Virt(res) => res.get(),
        }
    }
    fn set(&self, soft: u64, hard: u64) -> std::io::Result<()> {
        match self {
            Resource::Phy(res) => res.set(soft, hard),
            Resource::Virt(res) => res.set(soft, hard),
        }
    }
    fn is_supported(&self) -> bool {
        match self {
            Resource::Phy(res) => res.is_supported(),
            Resource::Virt(res) => res.is_supported(),
        }
    }
}

#[derive(Clone, Copy)]
struct ResourceDescription {
    resource: Resource,
    help: &'static str,
    description: &'static str,
    short: char,
    unit: Unit,
}

impl ResourceDescription {
    const SBSIZE: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::SBSIZE),
        help: "the socket buffer size",
        description: "socket buffer size",
        short: 'b',
        unit: Unit::Bytes,
    };
    const CORE: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::CORE),
        help: "the maximum size of core files created",
        description: "core file size",
        short: 'c',
        unit: Unit::Block,
    };
    const DATA: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::DATA),
        help: "the maximum size of a process's data segment",
        description: "data seg size",
        short: 'd',
        unit: Unit::KBytes,
    };
    const NICE: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::NICE),
        help: "the maximum scheduling priority (`nice`)",
        description: "scheduling priority",
        short: 'e',
        unit: Unit::Number,
    };
    const FSIZE: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::FSIZE),
        help: "the maximum size of files written by the shell and its children",
        description: "file size",
        short: 'f',
        unit: Unit::Block,
    };
    const SIGPENDING: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::SIGPENDING),
        help: "the maximum number of pending signals",
        description: "pending signals",
        short: 'i',
        unit: Unit::Number,
    };
    const MEMLOCK: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::MEMLOCK),
        help: "the maximum size a process may lock into memory",
        description: "max locked memory",
        short: 'l',
        unit: Unit::KBytes,
    };
    const KQUEUES: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::KQUEUES),
        help: "the maximum number of kqueues allocated for this process",
        description: "max kqueues",
        short: 'k',
        unit: Unit::Number,
    };
    const RSS: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::RSS),
        help: "the maximum resident set size",
        description: "max memory size",
        short: 'm',
        unit: Unit::KBytes,
    };
    const NOFILE: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::NOFILE),
        help: "the maximum number of open file descriptors",
        description: "open files",
        short: 'n',
        unit: Unit::Number,
    };
    const MSGQUEUE: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::MSGQUEUE),
        help: "the maximum number of bytes in POSIX message queues",
        description: "POSIX message queues",
        short: 'q',
        unit: Unit::Bytes,
    };
    const PIPE: ResourceDescription = ResourceDescription {
        resource: Resource::Virt(Virtual::Pipe),
        help: "the pipe buffer size",
        description: "pipe size",
        short: 'p',
        unit: Unit::HalfKBytes,
    };
    const RTPRIO: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::RTPRIO),
        help: "the maximum real-time scheduling priority",
        description: "real-time priority",
        short: 'r',
        unit: Unit::Number,
    };
    const STACK: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::STACK),
        help: "the maximum stack size",
        description: "stack size",
        short: 's',
        unit: Unit::KBytes,
    };
    const CPU: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::CPU),
        help: "the maximum amount of cpu time in seconds",
        description: "cpu time",
        short: 't',
        unit: Unit::Seconds,
    };
    const NPROC: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::NPROC),
        help: "the maximum number of user processes",
        description: "max user processes",
        short: 'u',
        unit: Unit::Number,
    };
    const VMEM: ResourceDescription = ResourceDescription {
        resource: Resource::Phy(rlimit::Resource::AS),
        help: "the size of virtual memory",
        description: "virtual memory",
        short: 'v',
        unit: Unit::KBytes,
    };

    fn get(&self, hard: bool) -> std::io::Result<String> {
        let (soft_limit, hard_limit) = self.resource.get()?;
        let val = if hard { hard_limit } else { soft_limit };

        if val == rlimit::INFINITY {
            Ok("unlimited".into())
        } else {
            Ok(format!("{val}"))
        }
    }

    fn set(&self, set_hard: bool, value: LimitValue) -> std::io::Result<()> {
        let (soft, hard) = self.resource.get()?;
        let value = match value {
            LimitValue::Soft => soft,
            LimitValue::Hard => hard,
            LimitValue::Unlimited => rlimit::INFINITY,
            LimitValue::Value(v) => v,
            LimitValue::Unset => return Ok(()),
        };

        if set_hard {
            self.resource.set(soft, value)
        } else {
            self.resource.set(value, hard)
        }
    }

    /// Print either soft or hard limit
    fn print(&self, hard: bool) {
        if !self.resource.is_supported() {
            return;
        }
        let unit = match self.unit {
            Unit::Block => format!("(block, -{})", self.short),
            Unit::Bytes => format!("(bytes, -{})", self.short),
            Unit::HalfKBytes => format!("(512 bytes, -{})", self.short),
            Unit::KBytes => format!("(kbytes, -{})", self.short),
            Unit::Number => format!("(-{})", self.short),
            Unit::Seconds => format!("(seconds, -{})", self.short),
        };
        let resource = self.get(hard).unwrap_or_else(|e| format!("{e}"));
        println!("{:<26}{:>16} {}", self.description, unit, resource);
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
    fn into_resettable(self) -> clap::builder::Resettable<StyledStr> {
        clap::builder::Resettable::Value(self.help().into())
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
            "" => LimitValue::Unset,
            "unlimited" => LimitValue::Unlimited,
            "soft" => LimitValue::Soft,
            "hard" => LimitValue::Hard,
            _ => LimitValue::Value(s.parse()?),
        };
        Ok(v)
    }
}

/// Modify shell resource limits.
///
/// Provides control over the resources available to the shell and processes
/// it creates, on systems that allow such control.
#[derive(Parser, Debug)]
pub(crate) struct ULimitCommand {
    /// use the `soft` resource limit
    #[arg(short = 'S')]
    soft: bool,
    /// use the `hard` resource limit
    #[arg(short = 'H')]
    hard: bool,
    /// all current limits are reported
    #[arg(short = 'a')]
    all: bool,
    /// the maximum socket buffer size
    #[arg(short = 'b', default_missing_value = "", num_args(0..=1), help = ResourceDescription::SBSIZE)]
    sbsize: Option<LimitValue>,
    /// the maximum size of core files created
    #[arg(short = 'c', default_missing_value = "", num_args(0..=1), help = ResourceDescription::CORE)]
    core: Option<LimitValue>,
    /// the maximum size of a process's data segment
    #[arg(short = 'd', default_missing_value = "", num_args(0..=1), help = ResourceDescription::DATA)]
    data: Option<LimitValue>,
    /// the maximum scheduling priority (`nice`)
    #[arg(short = 'e', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NICE)]
    nice: Option<LimitValue>,
    /// the maximum size of files written by the shell and its children
    #[arg(short = 'f', default_missing_value = "", num_args(0..=1), help = ResourceDescription::FSIZE)]
    file_size: Option<LimitValue>,
    /// the maximum number of pending signals
    #[arg(short = 'i', default_missing_value = "", num_args(0..=1), help = ResourceDescription::SIGPENDING)]
    sigpending: Option<LimitValue>,
    /// the maximum size a process may lock into memory
    #[arg(short = 'l', default_missing_value = "", num_args(0..=1), help = ResourceDescription::MEMLOCK)]
    memlock: Option<LimitValue>,
    /// the maximum number of kqueues allocated for this process
    #[arg(short = 'k', default_missing_value = "", num_args(0..=1), help = ResourceDescription::KQUEUES)]
    kqueues: Option<LimitValue>,
    /// the maximum resident set size
    #[arg(short = 'm', default_missing_value = "", num_args(0..=1), help = ResourceDescription::RSS)]
    rss: Option<LimitValue>,
    /// the maximum number of open file descriptors
    #[arg(short = 'n', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NOFILE)]
    file_open: Option<LimitValue>,
    /// the pipe buffer size
    #[arg(short = 'p', default_missing_value = "", num_args(0..=1), help = ResourceDescription::PIPE)]
    pipe: Option<LimitValue>,
    /// the maximum number of bytes in POSIX message queues
    #[arg(short = 'q', default_missing_value = "", num_args(0..=1), help = ResourceDescription::MSGQUEUE)]
    msgqueue: Option<LimitValue>,
    /// the maximum real-time scheduling priority
    #[arg(short = 'r', default_missing_value = "", num_args(0..=1), help = ResourceDescription::RTPRIO)]
    rtprio: Option<LimitValue>,
    /// the maximum stack size
    #[arg(short = 's', default_missing_value = "", num_args(0..=1), help = ResourceDescription::STACK)]
    stack: Option<LimitValue>,
    /// the maximum amount of cpu time in seconds
    #[arg(short = 't', default_missing_value = "", num_args(0..=1), help = ResourceDescription::CPU)]
    cpu: Option<LimitValue>,
    /// the size of virtual memory
    #[arg(short = 'u', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NPROC)]
    nproc: Option<LimitValue>,
    /// the size of virtual memory
    #[arg(short = 'v', default_missing_value = "", num_args(0..=1), help = ResourceDescription::VMEM)]
    vmem: Option<LimitValue>,
    /// Unimplemented
    #[arg(short = 'x', default_missing_value = "", num_args(0..=1))]
    file_lock: Option<LimitValue>,
    /// Unimplemented
    #[arg(short = 'P', default_missing_value = "", num_args(0..=1))]
    npts: Option<LimitValue>,
    /// Unimplemented
    #[arg(short = 'R', default_missing_value = "", num_args(0..=1))]
    rttime: Option<LimitValue>,
    /// Unimplemented
    #[arg(short = 'T', default_missing_value = "", num_args(0..=1))]
    threads: Option<LimitValue>,

    /// argument for the implicit limit (`-f`)
    limit: Option<LimitValue>,
}

impl builtins::Command for ULimitCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let exit_code = builtins::ExitCode::Success;
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

        if self.threads.is_some()
            || self.rttime.is_some()
            || self.npts.is_some()
            || self.file_lock.is_some()
        {
            return crate::error::unimp("Limit unimplemented");
        }

        set_or_get(self.sbsize, ResourceDescription::SBSIZE);
        set_or_get(self.core, ResourceDescription::CORE);
        set_or_get(self.data, ResourceDescription::DATA);
        set_or_get(self.file_size, ResourceDescription::FSIZE);
        set_or_get(self.sigpending, ResourceDescription::SIGPENDING);
        set_or_get(self.kqueues, ResourceDescription::KQUEUES);
        set_or_get(self.memlock, ResourceDescription::MEMLOCK);
        set_or_get(self.rss, ResourceDescription::RSS);
        set_or_get(self.file_open, ResourceDescription::NOFILE);
        set_or_get(self.pipe, ResourceDescription::PIPE);
        set_or_get(self.nice, ResourceDescription::NICE);
        set_or_get(self.msgqueue, ResourceDescription::MSGQUEUE);
        set_or_get(self.rtprio, ResourceDescription::RTPRIO);
        set_or_get(self.stack, ResourceDescription::STACK);
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
            println!("{}", resources_to_get[0].get(self.hard)?);
        } else {
            for resource in resources_to_get {
                resource.print(self.hard);
            }
        }

        Ok(exit_code)
    }
}
