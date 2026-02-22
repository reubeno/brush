use brush_core::{ExecutionResult, builtins};
use clap::Parser;
use std::io::Write;
use std::path::{Path, PathBuf};

/// List directory contents.
///
/// Displays files and directories, similar to the external `ls` utility but
/// implemented as a shell builtin. Supports common options for controlling
/// output format and sorting.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
pub(crate) struct LsCommand {
    /// Show hidden files (entries starting with '.')
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Use a long listing format (permissions, size, date, name)
    #[arg(short = 'l')]
    long: bool,

    /// Print human-readable sizes (e.g. 1K, 234M) in long format
    #[arg(short = 'h', long = "human-readable")]
    human_readable: bool,

    /// Reverse the sort order
    #[arg(short = 'r', long = "reverse")]
    reverse: bool,

    /// Sort by file size, largest first
    #[arg(short = 'S')]
    sort_by_size: bool,

    /// Sort by modification time, newest first
    #[arg(short = 't')]
    sort_by_time: bool,

    /// List one file per line
    #[arg(short = '1')]
    one_per_line: bool,

    /// List directories recursively
    #[arg(short = 'R', long = "recursive")]
    recursive: bool,

    /// Paths to list. Defaults to the current directory.
    #[arg(trailing_var_arg = true)]
    paths: Vec<String>,
}

/// A single directory entry with cached metadata for sorting.
struct DirEntry {
    name: String,
    path: PathBuf,
    metadata: Option<std::fs::Metadata>,
}

impl DirEntry {
    fn from_dir_entry(entry: std::fs::DirEntry) -> Self {
        let metadata = entry.metadata().ok();
        let name = entry.file_name().to_string_lossy().into_owned();
        Self {
            name,
            path: entry.path(),
            metadata,
        }
    }

    fn is_dir(&self) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(std::fs::Metadata::is_dir)
    }
}

impl builtins::Command for LsCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let cwd = context.shell.working_dir().to_owned();

        let targets: Vec<PathBuf> = if self.paths.is_empty() {
            vec![cwd.clone()]
        } else {
            self.paths.iter().map(|p| cwd.join(p)).collect()
        };

        let multiple_targets = targets.len() > 1 || self.recursive;
        let mut first = true;

        for target in &targets {
            if !target.exists() {
                writeln!(
                    context.stderr(),
                    "ls: cannot access '{}': No such file or directory",
                    target.display()
                )?;
                continue;
            }

            if target.is_file() {
                self.print_file_entry(target, &cwd, &context)?;
                continue;
            }

            if multiple_targets {
                if !first {
                    writeln!(context.stdout())?;
                }
                writeln!(context.stdout(), "{}:", target.display())?;
            }
            first = false;

            self.list_directory(target, &cwd, &context)?;
        }

        Ok(ExecutionResult::success())
    }
}

impl LsCommand {
    fn list_directory<SE: brush_core::ShellExtensions>(
        &self,
        dir: &Path,
        cwd: &Path,
        context: &brush_core::ExecutionContext<'_, SE>,
    ) -> Result<(), brush_core::Error> {
        let mut entries = self.read_entries(dir)?;
        self.sort_entries(&mut entries);

        if self.long || self.one_per_line {
            for entry in &entries {
                if self.long {
                    self.print_long(entry, context)?;
                } else {
                    writeln!(context.stdout(), "{}", entry.name)?;
                }
            }
        } else {
            self.print_columns(&entries, context)?;
        }

        if self.recursive {
            let subdirs: Vec<PathBuf> = entries
                .iter()
                .filter(|e| e.is_dir() && e.name != "." && e.name != "..")
                .map(|e| e.path.clone())
                .collect();
            for subdir in subdirs {
                writeln!(context.stdout())?;
                writeln!(context.stdout(), "{}:", subdir.display())?;
                self.list_directory(&subdir, cwd, context)?;
            }
        }

        Ok(())
    }

    fn read_entries(&self, dir: &Path) -> Result<Vec<DirEntry>, brush_core::Error> {
        let read_dir = std::fs::read_dir(dir).map_err(|e| {
            brush_core::Error::from(brush_core::ErrorKind::IoError(e))
        })?;

        let mut entries: Vec<DirEntry> = read_dir
            .filter_map(|entry| entry.ok())
            .map(DirEntry::from_dir_entry)
            .filter(|e| self.all || !e.name.starts_with('.'))
            .collect();

        // Default sort: alphabetical, case-insensitive
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        Ok(entries)
    }

    fn sort_entries(&self, entries: &mut [DirEntry]) {
        if self.sort_by_size {
            entries.sort_by(|a, b| {
                let sa = a.metadata.as_ref().map_or(0, |m| m.len());
                let sb = b.metadata.as_ref().map_or(0, |m| m.len());
                sb.cmp(&sa) // largest first
            });
        } else if self.sort_by_time {
            entries.sort_by(|a, b| {
                let ta = a
                    .metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let tb = b
                    .metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                tb.cmp(&ta) // newest first
            });
        }

        if self.reverse {
            entries.reverse();
        }
    }

    fn print_file_entry<SE: brush_core::ShellExtensions>(
        &self,
        path: &Path,
        _cwd: &Path,
        context: &brush_core::ExecutionContext<'_, SE>,
    ) -> Result<(), brush_core::Error> {
        let name = path
            .file_name()
            .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
            .into_owned();
        let metadata = path.metadata().ok();
        let entry = DirEntry {
            name,
            path: path.to_owned(),
            metadata,
        };

        if self.long {
            self.print_long(&entry, context)?;
        } else {
            writeln!(context.stdout(), "{}", entry.name)?;
        }
        Ok(())
    }

    fn print_long<SE: brush_core::ShellExtensions>(
        &self,
        entry: &DirEntry,
        context: &brush_core::ExecutionContext<'_, SE>,
    ) -> Result<(), brush_core::Error> {
        let meta = match &entry.metadata {
            Some(m) => m,
            None => {
                writeln!(context.stdout(), "?????????? ? ? ? {}", entry.name)?;
                return Ok(());
            }
        };

        let file_type = if meta.is_dir() {
            'd'
        } else if meta.is_symlink() {
            'l'
        } else {
            '-'
        };

        let perms = format_permissions(meta);

        let size = if self.human_readable {
            format_human_size(meta.len())
        } else {
            meta.len().to_string()
        };

        let modified = meta
            .modified()
            .ok()
            .map(format_system_time)
            .unwrap_or_else(|| "?".to_string());

        writeln!(
            context.stdout(),
            "{file_type}{perms} {size:>8} {modified} {}",
            entry.name
        )?;

        Ok(())
    }

    fn print_columns<SE: brush_core::ShellExtensions>(
        &self,
        entries: &[DirEntry],
        context: &brush_core::ExecutionContext<'_, SE>,
    ) -> Result<(), brush_core::Error> {
        if entries.is_empty() {
            return Ok(());
        }

        // Determine terminal width; fall back to 80.
        let term_width: usize = context
            .shell
            .env()
            .get("COLUMNS")
            .and_then(|(_, var)| {
                var.value()
                    .to_cow_str(context.shell)
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or(80);

        let max_name_len = entries.iter().map(|e| e.name.len()).max().unwrap_or(1);
        let col_width = max_name_len + 2; // padding between columns
        let num_cols = (term_width / col_width).max(1);

        for (i, entry) in entries.iter().enumerate() {
            if i > 0 && i % num_cols == 0 {
                writeln!(context.stdout())?;
            }
            write!(context.stdout(), "{:<width$}", entry.name, width = col_width)?;
        }
        writeln!(context.stdout())?;

        Ok(())
    }
}

/// Format file permissions as a 9-character rwx string.
fn format_permissions(meta: &std::fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        let mut s = String::with_capacity(9);
        // owner
        s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o100 != 0 { 'x' } else { '-' });
        // group
        s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o010 != 0 { 'x' } else { '-' });
        // other
        s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o001 != 0 { 'x' } else { '-' });
        s
    }
    #[cfg(not(unix))]
    {
        let perms = meta.permissions();
        if perms.readonly() {
            "r--r--r--".to_string()
        } else {
            "rw-rw-rw-".to_string()
        }
    }
}

/// Format a `SystemTime` as a short UTC date string (e.g. `2025-01-07 14:30`).
fn format_system_time(time: std::time::SystemTime) -> String {
    let secs = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day, hour, minute) = unix_secs_to_utc(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Convert seconds since Unix epoch to (year, month, day, hour, minute) in UTC.
///
/// Civil date algorithm from Howard Hinnant:
/// <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>
fn unix_secs_to_utc(secs: u64) -> (u64, u64, u64, u64, u64) {
    let minute = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let total_days = secs / 86400;

    let z = total_days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, hour, minute)
}

/// Format a byte count in human-readable form (K, M, G, T).
fn format_human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T", "P"];
    if bytes == 0 {
        return "0B".to_string();
    }
    let mut size = bytes as f64;
    for unit in UNITS {
        if size < 1024.0 {
            return if size.fract() < 0.05 || *unit == "B" {
                format!("{}{unit}", size as u64)
            } else {
                format!("{size:.1}{unit}")
            };
        }
        size /= 1024.0;
    }
    format!("{size:.1}P")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_human_size() {
        assert_eq!(format_human_size(0), "0B");
        assert_eq!(format_human_size(100), "100B");
        assert_eq!(format_human_size(1024), "1K");
        assert_eq!(format_human_size(1536), "1.5K");
        assert_eq!(format_human_size(1_048_576), "1M");
        assert_eq!(format_human_size(1_073_741_824), "1G");
    }
}
