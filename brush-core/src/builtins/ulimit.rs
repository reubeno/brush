use clap::{
    builder::{IntoResettable, StyledStr},
    Parser,
};
use rlimit::Resource;
use std::str::FromStr;

use crate::{builtins, commands};

#[derive(Clone, Copy)]
enum Unit {
    Block,
    Bytes,
    KBytes,
    Number,
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
        resource: Resource::SBSIZE,
        help: "the socket buffer size",
        description: "socket buffer size",
        short: 'b',
        unit: Unit::Bytes,
    };
    const CORE: ResourceDescription = ResourceDescription {
        resource: Resource::CORE,
        help: "the maximum size of core files created",
        description: "core file size",
        short: 'c',
        unit: Unit::Block,
    };
    const DATA: ResourceDescription = ResourceDescription {
        resource: Resource::DATA,
        help: "the maximum size of a process's data segment",
        description: "data seg size",
        short: 'd',
        unit: Unit::KBytes,
    };
    const FSIZE: ResourceDescription = ResourceDescription {
        resource: Resource::FSIZE,
        help: "the maximum size of files written by the shell and its children",
        description: "file size",
        short: 'f',
        unit: Unit::Block,
    };
    const SIGPENDING: ResourceDescription = ResourceDescription {
        resource: Resource::SIGPENDING,
        help: "the maximum number of pending signals",
        description: "pending signals",
        short: 'i',
        unit: Unit::Number,
    };
    const MEMLOCK: ResourceDescription = ResourceDescription {
        resource: Resource::MEMLOCK,
        help: "the maximum size a process may lock into memory",
        description: "max locked memory",
        short: 'l',
        unit: Unit::KBytes,
    };
    const MSGQUEUE: ResourceDescription = ResourceDescription {
        resource: Resource::MSGQUEUE,
        help: "the maximum number of bytes in POSIX message queues",
        description: "POSIX message queues",
        short: 'q',
        unit: Unit::Bytes,
    };
    const RSS: ResourceDescription = ResourceDescription {
        resource: Resource::RSS,
        help: "the maximum resident set size",
        description: "max memory size",
        short: 'm',
        unit: Unit::KBytes,
    };
    const NOFILE: ResourceDescription = ResourceDescription {
        resource: Resource::NOFILE,
        help: "the maximum number of open file descriptors",
        description: "open files",
        short: 'n',
        unit: Unit::Number,
    };
    const NICE: ResourceDescription = ResourceDescription {
        resource: Resource::NICE,
        help: "the maximum scheduling priority (`nice`)",
        description: "scheduling priority",
        short: 'e',
        unit: Unit::Number,
    };
    const KQUEUES: ResourceDescription = ResourceDescription {
        resource: Resource::KQUEUES,
        help: "the maximum number of kqueues allocated for this process",
        description: "max kqueues",
        short: 'k',
        unit: Unit::Number,
    };

    fn get(&self, hard: bool) -> std::io::Result<String> {
        let val = if hard {
            self.resource.get_hard()?
        } else {
            self.resource.get_soft()?
        };

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
            Unit::KBytes => format!("(kbytes, -{})", self.short),
            Unit::Number => format!("(-{})", self.short),
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
    #[arg(short)]
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
    /// the maximum number of bytes in POSIX message queues
    #[arg(short = 'q', default_missing_value = "", num_args(0..=1), help = ResourceDescription::MSGQUEUE)]
    msgqueue: Option<LimitValue>,
    /// the maximum scheduling priority (`nice`)
    #[arg(short = 'e', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NICE)]
    nice: Option<LimitValue>,
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

        set_or_get(self.sbsize, ResourceDescription::SBSIZE);
        set_or_get(self.core, ResourceDescription::CORE);
        set_or_get(self.data, ResourceDescription::DATA);
        set_or_get(self.file_size, ResourceDescription::FSIZE);
        set_or_get(self.sigpending, ResourceDescription::SIGPENDING);
        set_or_get(self.kqueues, ResourceDescription::KQUEUES);
        set_or_get(self.memlock, ResourceDescription::MEMLOCK);
        set_or_get(self.rss, ResourceDescription::RSS);
        set_or_get(self.file_open, ResourceDescription::NOFILE);
        set_or_get(self.nice, ResourceDescription::NICE);
        set_or_get(self.msgqueue, ResourceDescription::MSGQUEUE);

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
