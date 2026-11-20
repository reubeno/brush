//! brush, with fake bundled commands registered.
//!
//! The bundled-command shim runs a bundled command by re-executing its own
//! binary as `<exe> --invoke-bundled NAME ARGS...`, so testing it end to end
//! needs a brush binary that has bundled commands registered. The real one has
//! none unless built with `experimental-bundled-coreutils`.

use std::ffi::OsString;
use std::io::{BufRead as _, Write as _};
use std::path::Path;

use brush_shell::bundled::{self, BundledFn};

fn main() {
    let commands: [(&str, BundledFn); 5] = [
        ("bundled-echo", echo),
        ("bundled-exit", exit),
        ("bundled-cat", cat),
        ("bundled-argv0", argv0),
        // Shares its name with a brush builtin, which must win.
        ("echo", shadowed_echo),
    ];
    bundled::install(
        commands
            .into_iter()
            .map(|(name, f)| (name.to_owned(), f))
            .collect(),
    );

    // Installing first makes brush's own (empty) providers a no-op.
    brush_shell::entry::run();
}

/// Writes each argument, `argv[0]` included, on a line of its own in brackets.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a bundled command's signature is fixed"
)]
fn echo(args: Vec<OsString>) -> i32 {
    let mut stdout = std::io::stdout().lock();
    for arg in &args {
        if writeln!(stdout, "[{}]", arg.to_string_lossy()).is_err() {
            return 1;
        }
    }
    0
}

/// Says on stderr which status it's exiting with, then exits with the status
/// given as its first argument.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a bundled command's signature is fixed"
)]
fn exit(args: Vec<OsString>) -> i32 {
    let code = args
        .get(1)
        .and_then(|a| a.to_str())
        .and_then(|a| a.parse().ok())
        .unwrap_or(99);
    eprintln!("bundled-exit: {code}");
    code
}

/// Copies stdin to stdout, prefixing each line with `cat: `.
fn cat(_args: Vec<OsString>) -> i32 {
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { return 1 };
        if writeln!(stdout, "cat: {line}").is_err() {
            return 1;
        }
    }
    0
}

/// Writes the file name of the process's own `argv[0]`.
fn argv0(_args: Vec<OsString>) -> i32 {
    let argv0 = std::env::args_os().next().unwrap_or_default();
    let name = Path::new(&argv0).file_name().unwrap_or_default();
    println!("{}", name.to_string_lossy());
    0
}

fn shadowed_echo(_args: Vec<OsString>) -> i32 {
    println!("bundled echo ran instead of the builtin");
    0
}
