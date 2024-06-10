use std::{ffi::OsStr, fmt::Display, os::unix::process::CommandExt, process::Stdio};

use brush_parser::ast;
use command_fds::{CommandFdExt, FdMapping};

use crate::{
    error,
    openfiles::{OpenFile, OpenFiles},
    Shell,
};

/// Represents the context for executing a command.
pub struct ExecutionContext<'a> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell,
    /// The name of the command being executed.    
    pub command_name: String,
    /// The open files tracked by the current context.
    pub open_files: crate::openfiles::OpenFiles,
}

impl ExecutionContext<'_> {
    /// Returns the standard input file; usable with `write!` et al.
    pub fn stdin(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&0).unwrap().try_dup().unwrap()
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&1).unwrap().try_dup().unwrap()
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> crate::openfiles::OpenFile {
        self.open_files.files.get(&2).unwrap().try_dup().unwrap()
    }
}

/// An argument to a command.
#[derive(Clone, Debug)]
pub enum CommandArg {
    /// A simple string argument.
    String(String),
    /// An assignment/declaration; typically treated as a string, but will
    /// be specially handled by a limited set of built-in commands.
    Assignment(ast::Assignment),
}

impl Display for CommandArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandArg::String(s) => f.write_str(s),
            CommandArg::Assignment(a) => write!(f, "{a}"),
        }
    }
}

impl From<String> for CommandArg {
    fn from(s: String) -> Self {
        CommandArg::String(s)
    }
}

impl From<&String> for CommandArg {
    fn from(value: &String) -> Self {
        CommandArg::String(value.clone())
    }
}

pub(crate) fn compose_std_command<S: AsRef<OsStr>>(
    shell: &mut Shell,
    command_name: &str,
    argv0: &str,
    args: &[S],
    mut open_files: OpenFiles,
    empty_env: bool,
) -> Result<(std::process::Command, Option<String>), error::Error> {
    let mut cmd = std::process::Command::new(command_name);

    // Override argv[0].
    cmd.arg0(argv0);

    // Pass through args.
    for arg in args {
        cmd.arg(arg);
    }

    // Use the shell's current working dir.
    cmd.current_dir(shell.working_dir.as_path());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    if !empty_env {
        for (name, var) in shell.env.iter() {
            if var.is_exported() {
                let value_as_str = var.value().to_cow_string();
                cmd.env(name, value_as_str.as_ref());
            }
        }
    }

    // Redirect stdin, if applicable.
    let mut stdin_here_doc = None;
    if let Some(stdin_file) = open_files.files.remove(&0) {
        if let OpenFile::HereDocument(doc) = &stdin_file {
            stdin_here_doc = Some(doc.clone());
        }

        let as_stdio: Stdio = stdin_file.into();
        cmd.stdin(as_stdio);
    }

    // Redirect stdout, if applicable.
    match open_files.files.remove(&1) {
        Some(OpenFile::Stdout) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match open_files.files.remove(&2) {
        Some(OpenFile::Stderr) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.into();
            cmd.stderr(as_stdio);
        }
    }

    // Inject any other fds.
    #[cfg(unix)]
    {
        let fd_mappings = open_files
            .files
            .into_iter()
            .map(|(child_fd, open_file)| FdMapping {
                child_fd: i32::try_from(child_fd).unwrap(),
                parent_fd: open_file.into_owned_fd().unwrap(),
            })
            .collect();
        cmd.fd_mappings(fd_mappings)
            .map_err(|_e| error::Error::ChildCreationFailure)?;
    }
    #[cfg(not(unix))]
    {
        if !open_files.files.is_empty() {
            return error::unimp("fd redirections on non-Unix platform");
        }
    }

    Ok((cmd, stdin_here_doc))
}
