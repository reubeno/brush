//! Process management utilities

use std::path::Path;

pub(crate) use crate::sys::tokio_process::*;

/// Creates a command to run the executable at `program` as a child of the shell, starting in
/// `working_dir` (the shell's working directory) rather than the host process's. If
/// `working_dir` is empty (the shell doesn't know where it is), the child starts in the host
/// process's.
///
/// `program` is a path the shell has already found -- absolute, or relative to `working_dir`
/// -- never a name to search for.
///
/// Windows looks a relative path up against the host process's working directory, not the
/// child's, so `program` is resolved against `working_dir` first.
///
/// # Arguments
///
/// * `program` - The path of the executable to run.
/// * `working_dir` - The directory the child starts in.
pub(crate) fn create_command(
    program: impl AsRef<Path>,
    working_dir: &crate::ResolvedPath,
) -> std::process::Command {
    let mut command = std::process::Command::new(working_dir.join(program.as_ref()).as_path());
    if !working_dir.is_empty() {
        command.current_dir(working_dir);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(path: &str) -> crate::ResolvedPath {
        crate::ResolvedPath::try_from(std::path::PathBuf::from(path)).unwrap_or_default()
    }

    #[test]
    fn relative_program_is_resolved_against_the_working_directory() {
        let working_dir = resolved(r"C:\work");

        for (program, expected) in [
            (r"prog.exe", r"C:\work\prog.exe"),
            (r"bin\prog.exe", r"C:\work\bin\prog.exe"),
            (r".\prog.exe", r"C:\work\.\prog.exe"),
            (r"D:prog.exe", r"D:\prog.exe"),
            (r"C:\bin\prog.exe", r"C:\bin\prog.exe"),
        ] {
            let command = create_command(program, &working_dir);
            assert_eq!(command.get_program(), expected);
            assert_eq!(command.get_current_dir(), Some(Path::new(r"C:\work")));
        }
    }
}
