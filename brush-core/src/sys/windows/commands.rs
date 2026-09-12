//! Command execution utilities.

use std::{
    ffi::{OsStr, OsString},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        process::CommandExt as _,
    },
    path::Path,
};

pub use crate::sys::stubs::commands::{
    CommandExt, CommandFdInjectionExt, CommandFgControlExt, ExitStatusExt,
};

/// Sets the arguments to be passed to the given command.
///
/// Windows has no `argv` array; a child process receives a single command-line
/// string and is responsible for parsing it itself. The standard library only
/// wraps an argument in double quotes when it is empty or contains whitespace
/// or a quote, which leaves every other argument exposed to a second round of
/// interpretation in the child.
///
/// That second round is not hypothetical: programs built on the MSYS2/Cygwin
/// runtime (for example, the tools shipped with Git for Windows) re-expand the
/// arguments they receive from a native parent, applying glob, brace, and tilde
/// expansion along with quote removal. Words that the shell has already
/// finished expanding are silently corrupted as a result.
///
/// To keep the child's view of its arguments identical to the shell's, each
/// argument is explicitly quoted using the rules understood by
/// `CommandLineToArgvW` and the Microsoft C runtime. Native programs are
/// unaffected by the added quotes, since they strip them while parsing.
///
/// # Arguments
///
/// * `cmd` - The command to set arguments on.
/// * `args` - The arguments to pass to the command.
pub fn set_args<S: AsRef<OsStr>>(cmd: &mut std::process::Command, args: &[S]) {
    // Arguments destined for `cmd.exe` are deliberately left alone; see
    // `uses_cmd_exe_parsing` for why.
    if uses_cmd_exe_parsing(cmd.get_program()) {
        cmd.args(args);
        return;
    }

    for arg in args {
        cmd.raw_arg(quote_arg(arg.as_ref()));
    }
}

/// Returns true if the given program's arguments will be parsed by `cmd.exe`.
///
/// Such arguments must be left to the standard library to encode, for two
/// reasons. First, `cmd.exe` does not accept quotes around its own options: a
/// command line of `cmd.exe "/c" "ver"` fails outright. Second, the standard
/// library applies extra escaping when spawning a batch file (which it runs via
/// `cmd.exe`) to keep the child from interpreting metacharacters such as `%`;
/// bypassing that with a raw command line would reintroduce a command injection
/// hazard.
///
/// Neither `cmd.exe` nor a batch file is an MSYS2/Cygwin program, so no
/// protection is lost by deferring to the standard library here.
///
/// # Arguments
///
/// * `program` - The program the command will execute.
fn uses_cmd_exe_parsing(program: &OsStr) -> bool {
    let path = Path::new(program);
    let extension = path.extension().and_then(OsStr::to_str);

    // Batch files are launched through `cmd.exe`.
    if extension
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd"))
    {
        return true;
    }

    // `cmd.exe` itself, named with or without its extension.
    if extension.is_none_or(|ext| ext.eq_ignore_ascii_case("exe")) {
        return path
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| stem.eq_ignore_ascii_case("cmd"));
    }

    false
}

/// Quotes the given argument so that a child process parsing its command line
/// with `CommandLineToArgvW` semantics recovers the argument unmodified.
///
/// The argument is always quoted, even when quoting would not strictly be
/// required to preserve word boundaries. The quotes are what stop an
/// MSYS2/Cygwin child from re-expanding the argument.
///
/// # Arguments
///
/// * `arg` - The argument to quote.
fn quote_arg(arg: &OsStr) -> OsString {
    let backslash = u16::from(b'\\');
    let quote = u16::from(b'"');

    let mut quoted: Vec<u16> = Vec::with_capacity(arg.len() + 2);
    quoted.push(quote);

    let mut pending_backslashes = 0usize;
    for unit in arg.encode_wide() {
        if unit == backslash {
            pending_backslashes += 1;
        } else {
            if unit == quote {
                // Any backslashes immediately preceding a quote are treated as escape
                // characters, so they need doubling; the quote itself then needs escaping.
                quoted.extend(std::iter::repeat_n(backslash, pending_backslashes + 1));
            }
            pending_backslashes = 0;
        }

        quoted.push(unit);
    }

    // Backslashes at the end of the argument would otherwise escape the closing quote.
    quoted.extend(std::iter::repeat_n(backslash, pending_backslashes));
    quoted.push(quote);

    OsString::from_wide(quoted.as_slice())
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use anyhow::Result;

    use super::*;

    fn quoted(arg: &str) -> String {
        quote_arg(OsStr::new(arg)).to_string_lossy().into_owned()
    }

    fn args_of(cmd: &std::process::Command) -> Vec<String> {
        cmd.get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn test_quote_arg_quotes_ordinary_words() {
        assert_eq!(quoted("abc"), r#""abc""#);
        assert_eq!(quoted(""), r#""""#);
        assert_eq!(quoted("a b"), r#""a b""#);
        assert_eq!(quoted("-oE"), r#""-oE""#);
    }

    #[test]
    fn test_quote_arg_protects_msys_metacharacters() {
        // These are the characters an MSYS2/Cygwin child would otherwise re-expand.
        assert_eq!(quoted("[0-9]{3}"), r#""[0-9]{3}""#);
        assert_eq!(quoted("x{a,b}y"), r#""x{a,b}y""#);
        assert_eq!(quoted("*"), r#""*""#);
        assert_eq!(quoted("*.txt"), r#""*.txt""#);
        assert_eq!(quoted("~"), r#""~""#);
        assert_eq!(quoted("a'b"), r#""a'b""#);
        assert_eq!(quoted(r"MAC\([A-Fa-f0-9]{12}"), r#""MAC\([A-Fa-f0-9]{12}""#);
    }

    #[test]
    fn test_quote_arg_escapes_embedded_quotes() {
        assert_eq!(quoted(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quoted(r#"a\"b"#), r#""a\\\"b""#);
        assert_eq!(quoted(r#"he said "hi""#), r#""he said \"hi\"""#);
    }

    #[test]
    fn test_quote_arg_doubles_only_trailing_backslashes() {
        assert_eq!(quoted(r"C:\path"), r#""C:\path""#);
        assert_eq!(quoted(r"C:\path\"), r#""C:\path\\""#);
        assert_eq!(quoted(r"C:\path\\"), r#""C:\path\\\\""#);
    }

    #[test]
    fn test_uses_cmd_exe_parsing() {
        assert!(uses_cmd_exe_parsing(OsStr::new("cmd")));
        assert!(uses_cmd_exe_parsing(OsStr::new("cmd.exe")));
        assert!(uses_cmd_exe_parsing(OsStr::new("CMD.EXE")));
        assert!(uses_cmd_exe_parsing(OsStr::new(
            r"C:\Windows\System32\cmd.exe"
        )));
        assert!(uses_cmd_exe_parsing(OsStr::new(r"C:\scripts\build.bat")));
        assert!(uses_cmd_exe_parsing(OsStr::new(r"C:\scripts\build.CMD")));

        assert!(!uses_cmd_exe_parsing(OsStr::new("grep.exe")));
        assert!(!uses_cmd_exe_parsing(OsStr::new("mycmd.exe")));
        assert!(!uses_cmd_exe_parsing(OsStr::new("cmd.com")));
        assert!(!uses_cmd_exe_parsing(OsStr::new(
            r"C:\Program Files\Git\usr\bin\grep.exe"
        )));
    }

    #[test]
    fn test_set_args_quotes_args_for_ordinary_programs() {
        let mut cmd = std::process::Command::new("grep.exe");
        set_args(&mut cmd, &["-oE", "[0-9]{3}"]);

        assert_eq!(args_of(&cmd), vec![r#""-oE""#, r#""[0-9]{3}""#]);
    }

    #[test]
    fn test_set_args_leaves_cmd_exe_args_alone() {
        let mut cmd = std::process::Command::new("cmd.exe");
        set_args(&mut cmd, &["/c", "ver"]);

        assert_eq!(args_of(&cmd), vec!["/c", "ver"]);

        let mut cmd = std::process::Command::new(r"C:\scripts\build.bat");
        set_args(&mut cmd, &["%CD%"]);

        assert_eq!(args_of(&cmd), vec!["%CD%"]);
    }

    /// Verifies end-to-end that `cmd.exe` still works; blanket quoting would
    /// break it with `'"ver' is not recognized as an internal or external command`.
    #[test]
    fn test_cmd_exe_still_runs() -> Result<()> {
        let mut cmd = std::process::Command::new("cmd.exe");
        set_args(&mut cmd, &["/c", "echo hello world"]);

        let output = cmd.output()?;

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            "hello world"
        );

        Ok(())
    }

    /// Verifies end-to-end that a batch file still receives `%`-bearing arguments
    /// literally; encoding them into a raw command line would let `cmd.exe` expand
    /// them.
    #[test]
    fn test_batch_file_args_are_not_expanded() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script_path = dir.path().join("echo-arg.bat");
        std::fs::write(&script_path, "@echo off\r\necho GOT:[%~1]\r\n")?;

        let mut cmd = std::process::Command::new(&script_path);
        set_args(&mut cmd, &["%CD%"]);

        let output = cmd.output()?;

        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            "GOT:[%CD%]"
        );

        Ok(())
    }

    /// Verifies end-to-end that an MSYS2/Cygwin child receives arguments exactly as
    /// the shell expanded them. Skipped when no such program can be found.
    #[test]
    fn test_msys_child_receives_args_unmodified() -> Result<()> {
        let Some(printf_path) = find_msys_printf() else {
            return Ok(());
        };

        let cases = [
            "[0-9]{3}",
            r"MAC\([A-Fa-f0-9]{12}",
            "x{a,b}y",
            "*",
            "~",
            "a'b",
            r#"a"b"#,
            r"C:\path\",
            "a b",
        ];

        for case in cases {
            let mut cmd = std::process::Command::new(&printf_path);
            set_args(&mut cmd, &["<%s>\n", case]);

            let output = cmd.output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);

            assert_eq!(stdout.trim_end(), std::format!("<{case}>"));
        }

        Ok(())
    }

    /// Locates an MSYS2/Cygwin build of `printf`, if one is installed.
    fn find_msys_printf() -> Option<std::path::PathBuf> {
        // Git for Windows ships its MSYS2 tools here.
        let candidate = std::path::PathBuf::from(r"C:\Program Files\Git\usr\bin\printf.exe");
        candidate.is_file().then_some(candidate)
    }
}
