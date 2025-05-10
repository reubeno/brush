use clap::Parser;
use rlimit::Resource;
use std::str::FromStr;

use crate::{builtins, commands};

#[derive(Clone, Copy)]
enum Unit {
    Block,
    KBytes,
    Number,
}

#[derive(Clone, Copy)]
struct ResourceDescription {
    resource: Resource,
    description: &'static str,
    short: char,
    unit: Unit,
}

impl ResourceDescription {
    const CORE: ResourceDescription = ResourceDescription {
        resource: Resource::CORE,
        description: "core file size",
        short: 'c',
        unit: Unit::Block,
    };
    const FSIZE: ResourceDescription = ResourceDescription {
        resource: Resource::FSIZE,
        description: "file size",
        short: 'f',
        unit: Unit::Block,
    };
    const RSS: ResourceDescription = ResourceDescription {
        resource: Resource::RSS,
        description: "max memory size",
        short: 'm',
        unit: Unit::KBytes,
    };
    const NOFILE: ResourceDescription = ResourceDescription {
        resource: Resource::NOFILE,
        description: "open files",
        short: 'n',
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
    fn print(&self, hard: bool) -> std::io::Result<()> {
        let unit = match self.unit {
            Unit::Block => format!("(block, -{})", self.short),
            Unit::KBytes => format!("(kbytes, -{})", self.short),
            Unit::Number => format!("(-{})", self.short),
        };
        let resource = self.get(hard)?;
        println!("{:<26}{:>16} {}", self.description, unit, resource);
        Ok(())
    }
}

const ALL_RESOURCES: [ResourceDescription; 4] = [
    ResourceDescription::CORE,
    ResourceDescription::FSIZE,
    ResourceDescription::NOFILE,
    ResourceDescription::RSS,
];

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
    /// the maximum size of core files created
    #[arg(short, default_missing_value = "", num_args(0..=1))]
    core: Option<LimitValue>,
    /// the maximum size of files written by the shell and its children
    #[arg(short = 'f', default_missing_value = "", num_args(0..=1))]
    file_size: Option<LimitValue>,
    /// the maximum resident set size
    #[arg(short = 'm', default_missing_value = "", num_args(0..=1))]
    rss: Option<LimitValue>,
    /// the maximum number of open file descriptors
    #[arg(short = 'n', default_missing_value = "", num_args(0..=1))]
    file_open: Option<LimitValue>,

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

        let mut set_or_get = |val, descr| match val {
            Some(LimitValue::Unset) => resources_to_get.push(descr),
            Some(v) => resources_to_set.push((descr, v)),
            None => {}
        };

        set_or_get(self.core, ResourceDescription::CORE);
        set_or_get(self.file_size, ResourceDescription::FSIZE);
        set_or_get(self.rss, ResourceDescription::RSS);
        set_or_get(self.file_open, ResourceDescription::NOFILE);

        if self.all {
            resources_to_get = ALL_RESOURCES.into();
        }

        if !resources_to_get.is_empty() && !resources_to_set.is_empty() {
            return Err(crate::Error::InvalidArguments);
        }

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
                resource.print(self.hard)?;
            }
        }

        Ok(exit_code)
    }
}
