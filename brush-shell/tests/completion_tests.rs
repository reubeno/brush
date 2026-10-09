//! Completion integration tests for brush shell.

// For now, only compile this for Linux.
#![cfg(target_os = "linux")]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use assert_fs::prelude::*;
use brush_builtins::ShellBuilderExt;
use std::path::PathBuf;

/// Returns each candidate in `completions`.
fn candidate_texts(completions: &brush_core::completion::Completions) -> Vec<String> {
    completions.candidates.clone()
}

/// Returns the range of the line that the only candidate in `completions` replaces.
fn replaced_range(
    completions: &brush_core::completion::Completions,
) -> Result<std::ops::Range<usize>> {
    match completions.candidates.as_slice() {
        [_] => {
            let start = completions.insertion_index;
            Ok(start..start + completions.delete_count)
        }
        candidates => Err(anyhow::anyhow!(
            "expected one candidate, got {candidates:?}"
        )),
    }
}

/// A shell to complete in, with a temporary working directory.
struct TestShell {
    shell: brush_core::Shell,
    temp_dir: assert_fs::TempDir,
}

const DEFAULT_BASH_COMPLETION_SCRIPT: &str = "/usr/share/bash-completion/bash_completion";

impl TestShell {
    /// Returns a shell with native completion only.
    async fn new() -> Result<Self> {
        let mut shell = brush_core::Shell::builder()
            .profile(brush_core::ProfileLoadBehavior::Skip)
            .rc(brush_core::RcLoadBehavior::Skip)
            .default_builtins(brush_builtins::BuiltinSet::BashMode)
            // Don't leave external commands running when a test drops a completion.
            .kill_external_commands_on_drop(true)
            .build()
            .await?;

        let temp_dir = assert_fs::TempDir::new()?;
        shell.set_working_dir(temp_dir.path())?;

        Ok(Self { shell, temp_dir })
    }

    /// Returns a shell with bash-completion loaded.
    async fn with_bash_completion() -> Result<Self> {
        let mut test_shell = Self::new().await?;

        let exec_params = test_shell.shell.default_exec_params();
        let source_result = test_shell
            .shell
            .source_script(
                Self::find_bash_completion_script()?.as_path(),
                std::iter::empty::<String>(),
                &exec_params,
            )
            .await?;
        if !source_result.is_success() {
            return Err(anyhow::anyhow!("failed to source bash completion script"));
        }

        Ok(test_shell)
    }

    fn find_bash_completion_script() -> Result<PathBuf> {
        // See if an environmental override was provided.
        let script_path = std::env::var("BASH_COMPLETION_PATH").map_or_else(
            |_| PathBuf::from(DEFAULT_BASH_COMPLETION_SCRIPT),
            PathBuf::from,
        );

        if script_path.exists() {
            Ok(script_path)
        } else {
            Err(anyhow::anyhow!(
                "bash completion script not found: {}",
                script_path.display()
            ))
        }
    }

    /// Completes `line` at `pos`.
    async fn complete(
        &mut self,
        line: &str,
        pos: usize,
    ) -> Result<brush_core::completion::Completions> {
        Ok(self.shell.complete(line, pos).await?)
    }

    /// Completes `input` at its `|`, which marks the cursor (or at its end, if it has none).
    async fn complete_at_marker(
        &mut self,
        input: &str,
    ) -> Result<brush_core::completion::Completions> {
        let cursor = input.find('|').unwrap_or(input.len());
        let line = input.replacen('|', "", 1);
        self.complete(&line, cursor).await
    }

    /// Completes `line` at its end.
    async fn complete_end_of_line_full(
        &mut self,
        line: &str,
    ) -> Result<brush_core::completion::Completions> {
        self.complete(line, line.len()).await
    }

    /// Completes `line` at its end, returning the text of each candidate.
    async fn complete_end_of_line(&mut self, line: &str) -> Result<Vec<String>> {
        let completions = self.complete_end_of_line_full(line).await?;
        Ok(candidate_texts(&completions))
    }

    async fn run(&mut self, script: &str) -> Result<()> {
        let exec_params = self.shell.default_exec_params();
        self.shell
            .run_string(
                script.to_owned(),
                &brush_core::SourceInfo::default(),
                &exec_params,
            )
            .await?;
        Ok(())
    }

    fn get_var(&self, name: &str) -> Option<String> {
        self.shell
            .env()
            .get(name)
            .map(|(_, v)| v.value().to_cow_str(&self.shell).into_owned())
    }

    fn set_var(&mut self, name: &str, value: &str) -> Result<()> {
        self.shell
            .env_mut()
            .set_global(name, brush_core::ShellVariable::new(value))?;
        Ok(())
    }
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_relative_file_path() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;
    test_shell.temp_dir.child(".dot_item1").touch()?;
    test_shell.temp_dir.child("..dot_item2").touch()?;

    // Complete; expect to see the two files.
    let mut results = test_shell.complete_end_of_line("ls item").await?;

    assert_eq!(results, ["item1", "item2"]);

    results = test_shell.complete_end_of_line("ls .").await?;
    // Some versions of bash-completion filter out "." and ".." via
    // `-X '?(*/)@(.|..)'`; others don't. Accept either outcome.
    assert!(
        results == ["..dot_item2", ".dot_item1"]
            || results == [".", "..", "..dot_item2", ".dot_item1"],
        "unexpected completions for 'ls .': {results:?}"
    );

    results = test_shell.complete_end_of_line("ls ..").await?;
    assert_eq!(results, ["..", "..dot_item2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_relative_file_path_ignoring_case() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;
    test_shell
        .shell
        .options_mut()
        .case_insensitive_pathname_expansion = true;

    // Create file and dir.
    test_shell.temp_dir.child("ITEM1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls item").await?;

    assert_eq!(results, ["ITEM1", "item2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_relative_dir_path() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see just the dir.
    let results = test_shell.complete_end_of_line("cd item").await?;

    assert_eq!(results, ["item2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_under_empty_dir() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("empty").create_dir_all()?;

    // Complete; expect to see nothing.
    let results = test_shell.complete_end_of_line("ls empty/").await?;

    assert_eq!(results, Vec::<String>::new());

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_nonexistent_relative_path() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Complete; expect to see nothing.
    let results = test_shell.complete_end_of_line("ls item").await?;

    assert_eq!(results, Vec::<String>::new());

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_absolute_paths() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see just the dir.
    let input = std::format!("ls {}", test_shell.temp_dir.path().join("item").display());
    let results = test_shell.complete_end_of_line(input.as_str()).await?;

    assert_eq!(
        results,
        [
            test_shell
                .temp_dir
                .child("item1")
                .path()
                .display()
                .to_string(),
            test_shell
                .temp_dir
                .child("item2")
                .path()
                .display()
                .to_string(),
        ]
    );

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_with_var() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls $PWD/item").await?;

    assert_eq!(
        results,
        [
            test_shell
                .temp_dir
                .child("item1")
                .path()
                .display()
                .to_string(),
            test_shell
                .temp_dir
                .child("item2")
                .path()
                .display()
                .to_string(),
        ]
    );

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_with_tilde() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Set HOME to the temp dir so we can use ~ to reference it.
    test_shell.set_var(
        "HOME",
        test_shell
            .temp_dir
            .path()
            .to_string_lossy()
            .to_string()
            .as_str(),
    )?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls ~/item").await?;

    assert_eq!(
        results,
        [
            test_shell
                .temp_dir
                .child("item1")
                .path()
                .display()
                .to_string(),
            test_shell
                .temp_dir
                .child("item2")
                .path()
                .display()
                .to_string(),
        ]
    );

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_variable_names() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Set a few vars.
    test_shell.set_var("TESTVAR1", "")?;
    test_shell.set_var("TESTVAR2", "")?;

    // Complete.
    let results = test_shell.complete_end_of_line("echo $TESTVAR").await?;
    assert_eq!(results, ["$TESTVAR1", "$TESTVAR2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_variable_names_with_braces() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Set a few vars.
    test_shell.set_var("TESTVAR1", "")?;
    test_shell.set_var("TESTVAR2", "")?;

    // Complete.
    let results = test_shell.complete_end_of_line("echo ${TESTVAR").await?;
    assert_eq!(results, ["${TESTVAR1}", "${TESTVAR2}"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_help_topic() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Complete.
    let results = test_shell.complete_end_of_line("help expor").await?;
    assert_eq!(results, ["export"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_command_option() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Complete.
    let results = test_shell.complete_end_of_line("ls --hel").await?;
    assert_eq!(results, ["--help"]);

    Ok(())
}

/// Tests completion with some well-known programs that have been good manual test cases
/// for us in the past.
#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_args_to_well_known_programs() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete.
    let results = test_shell.complete_end_of_line("tar tvf ./item").await?;

    assert_eq!(results, ["./item2"]);

    Ok(())
}

/// Tests some 'find' completion.
#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_find_command() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Complete.
    let results = test_shell.complete_end_of_line("find . -na").await?;

    assert_eq!(results, ["-name"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_quoted_filenames() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    test_shell.temp_dir.child("item1 item2").touch()?;
    test_shell.temp_dir.child("item1'item2").touch()?;

    let mut results = test_shell.complete_end_of_line("ls item1\\ ").await?;
    assert_eq!(results, ["item1 item2"]);

    results = test_shell.complete_end_of_line("ls item1'").await?;
    assert_eq!(results, ["item1 item2", "item1'item2"]);

    results = test_shell.complete_end_of_line("ls item1").await?;
    assert_eq!(results, ["item1 item2", "item1'item2"]);

    results = test_shell.complete_end_of_line("ls 'item1 ").await?;
    assert_eq!(results, ["item1 item2"]);

    results = test_shell.complete_end_of_line("ls \"item1 ").await?;
    assert_eq!(results, ["item1 item2"]);

    Ok(())
}

/// Tests that interactive completion sets `COMP_KEY` and `COMP_TYPE` to 9 (TAB).
#[tokio::test(flavor = "multi_thread")]
async fn interactive_completion_sets_comp_key_and_comp_type() -> Result<()> {
    let mut shell = brush_core::Shell::builder()
        .profile(brush_core::ProfileLoadBehavior::Skip)
        .rc(brush_core::RcLoadBehavior::Skip)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        // Don't leave external commands running when a test drops a completion.
        .kill_external_commands_on_drop(true)
        .build()
        .await?;

    // Register a completion function that captures COMP_KEY and COMP_TYPE.
    let exec_params = shell.default_exec_params();
    let source_info = brush_core::SourceInfo::default();
    shell
        .run_string(
            r"
_test_comp() {
    CAPTURED_COMP_KEY=$COMP_KEY
    CAPTURED_COMP_TYPE=$COMP_TYPE
    COMPREPLY=(done)
}
complete -F _test_comp mycmd
"
            .to_string(),
            &source_info,
            &exec_params,
        )
        .await?;

    // Trigger interactive completion.
    let _completions = shell.complete("mycmd ", 6).await?;

    // Check the captured values.
    let comp_key = shell
        .env()
        .get("CAPTURED_COMP_KEY")
        .map(|(_, v)| v.value().to_cow_str(&shell).to_string());
    let comp_type = shell
        .env()
        .get("CAPTURED_COMP_TYPE")
        .map(|(_, v)| v.value().to_cow_str(&shell).to_string());

    assert_eq!(comp_key.as_deref(), Some("9"), "COMP_KEY should be 9 (TAB)");
    assert_eq!(
        comp_type.as_deref(),
        Some("9"),
        "COMP_TYPE should be 9 (TAB)"
    );

    Ok(())
}

/// Tests completing right after a word-break char (e.g. `--opt=`): like readline, the
/// word being completed is the empty one after the `=`, even though `COMP_WORDS`
/// still sees `=` as its own word.
#[tokio::test(flavor = "multi_thread")]
async fn complete_after_word_break_char() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run(
            r#"
_test_comp() {
    CAPTURED="[$2][$3][${COMP_WORDS[COMP_CWORD]}]"
    COMPREPLY=(root)
}
complete -F _test_comp mycmd
complete -W "root" mycmd2
"#,
        )
        .await?;

    let line = "mycmd --owner=";
    let completions = test_shell.complete(line, line.len()).await?;
    assert_eq!(replaced_range(&completions)?, line.len()..line.len());
    assert_eq!(candidate_texts(&completions), ["root"]);

    assert_eq!(
        test_shell.get_var("CAPTURED").as_deref(),
        Some("[][--owner][=]")
    );

    let line = "mycmd2 a:";
    let completions = test_shell.complete(line, line.len()).await?;
    assert_eq!(replaced_range(&completions)?, line.len()..line.len());
    assert_eq!(candidate_texts(&completions), ["root"]);

    // Cursor inside a run of word-break chars: still the empty word at the cursor.
    let line = "mycmd2 a:=val";
    let cursor = "mycmd2 a:".len();
    let completions = test_shell.complete(line, cursor).await?;
    assert_eq!(replaced_range(&completions)?, cursor..cursor);
    assert_eq!(candidate_texts(&completions), ["root"]);

    let line = "mycmd --owner:=val";
    let cursor = "mycmd --owner:".len();
    test_shell.complete(line, cursor).await?;
    assert_eq!(
        test_shell.get_var("CAPTURED").as_deref(),
        Some("[][--owner][:=]")
    );

    Ok(())
}

/// Checks what a completion function sees for the word being completed (`$2`), the one
/// before it (`$3`), and `COMP_WORDS`/`COMP_CWORD`, for each input with the cursor at
/// its `|`.
async fn check_completion_function_args(cases: &[(&str, &str)]) -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run(
            r#"
_test_comp() {
    local IFS=,
    CAPTURED="\$2=<$2> \$3=<$3> CWORD=$COMP_CWORD WORDS=<${COMP_WORDS[*]}>"
}
complete -F _test_comp mycmd
"#,
        )
        .await?;

    for (input, expected) in cases {
        test_shell.set_var("CAPTURED", "")?;
        test_shell.complete_at_marker(input).await?;
        assert_eq!(
            test_shell.get_var("CAPTURED").as_deref(),
            Some(*expected),
            "{input}"
        );
    }

    Ok(())
}

// Expected values in the tests below were captured from bash 5.3 by pressing Tab at the
// cursor.

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)]
async fn completion_function_args_match_bash() -> Result<()> {
    check_completion_function_args(&[
        ("mycmd aa|", "$2=<aa> $3=<mycmd> CWORD=1 WORDS=<mycmd,aa>"),
        ("mycmd aa |", "$2=<> $3=<aa> CWORD=2 WORDS=<mycmd,aa,>"),
        ("mycmd a|b", "$2=<a> $3=<mycmd> CWORD=1 WORDS=<mycmd,ab>"),
        ("mycmd |aa", "$2=<> $3=<mycmd> CWORD=1 WORDS=<mycmd,aa>"),
        (
            "mycmd aa|  bb",
            "$2=<aa> $3=<mycmd> CWORD=1 WORDS=<mycmd,aa,bb>",
        ),
        (
            "mycmd aa | bb",
            "$2=<> $3=<aa> CWORD=2 WORDS=<mycmd,aa,,bb>",
        ),
        ("mycmd aa |bb", "$2=<> $3=<aa> CWORD=2 WORDS=<mycmd,aa,bb>"),
        (
            "mycmd --owner=|",
            "$2=<> $3=<--owner> CWORD=2 WORDS=<mycmd,--owner,=>",
        ),
        (
            "mycmd --owner=|val",
            "$2=<> $3=<=> CWORD=3 WORDS=<mycmd,--owner,=,val>",
        ),
        (
            "mycmd --owner|=val",
            "$2=<--owner> $3=<--owner> CWORD=2 WORDS=<mycmd,--owner,=,val>",
        ),
        (
            "mycmd a:|=val",
            "$2=<> $3=<a> CWORD=2 WORDS=<mycmd,a,:=,val>",
        ),
        ("mycmd a:|", "$2=<> $3=<a> CWORD=2 WORDS=<mycmd,a,:>"),
        (
            "mycmd \"a b|",
            "$2=<a b> $3=<mycmd> CWORD=1 WORDS=<mycmd,\"a b>",
        ),
        (
            "mycmd 'a b|",
            "$2=<a b> $3=<mycmd> CWORD=1 WORDS=<mycmd,'a b>",
        ),
        (
            "mycmd a'b c|",
            "$2=<b c> $3=<mycmd> CWORD=1 WORDS=<mycmd,a'b c>",
        ),
        (
            "mycmd 'a b'|",
            "$2=<'a b'> $3=<mycmd> CWORD=1 WORDS=<mycmd,'a b'>",
        ),
        (
            "mycmd 'a b'c|",
            "$2=<'a b'c> $3=<mycmd> CWORD=1 WORDS=<mycmd,'a b'c>",
        ),
        (
            r"mycmd a\ b|",
            r"$2=<a\ b> $3=<mycmd> CWORD=1 WORDS=<mycmd,a\ b>",
        ),
        (
            "mycmd \"a:b|",
            "$2=<a:b> $3=<mycmd> CWORD=1 WORDS=<mycmd,\"a:b>",
        ),
        (
            "mycmd x=\"a b|",
            "$2=<a b> $3=<=> CWORD=3 WORDS=<mycmd,x,=,\"a b>",
        ),
        (
            "mycmd --opt='a b|",
            "$2=<a b> $3=<=> CWORD=3 WORDS=<mycmd,--opt,=,'a b>",
        ),
        (
            r"mycmd a\:b|",
            r"$2=<a\:b> $3=<mycmd> CWORD=1 WORDS=<mycmd,a\:b>",
        ),
        (
            r#"mycmd "a\"b c|"#,
            r#"$2=<a\"b c> $3=<mycmd> CWORD=1 WORDS=<mycmd,"a\"b c>"#,
        ),
        (
            "mycmd 'a:b' c:|",
            "$2=<> $3=<c> CWORD=3 WORDS=<mycmd,'a:b',c,:>",
        ),
        (
            "mycmd a\"b\"c:d|",
            "$2=<d> $3=<:> CWORD=3 WORDS=<mycmd,a\"b\"c,:,d>",
        ),
        ("mycmd a@b|", "$2=<@b> $3=<@> CWORD=3 WORDS=<mycmd,a,@,b>"),
        (
            "mycmd a:@b|",
            "$2=<@b> $3=<:@> CWORD=3 WORDS=<mycmd,a,:@,b>",
        ),
        ("mycmd a@|", "$2=<@> $3=<a> CWORD=2 WORDS=<mycmd,a,@>"),
        (
            r"mycmd a\@b|",
            r"$2=<a\@b> $3=<mycmd> CWORD=1 WORDS=<mycmd,a\@b>",
        ),
        (
            "mycmd \"a@b|",
            "$2=<a@b> $3=<mycmd> CWORD=1 WORDS=<mycmd,\"a@b>",
        ),
        (
            "mycmd $'a b|",
            "$2=<a b> $3=<mycmd> CWORD=1 WORDS=<mycmd,$'a b>",
        ),
        ("mycmd a >b|", "$2=<b> $3=<>> CWORD=3 WORDS=<mycmd,a,>,b>"),
        ("mycmd a>b|", "$2=<b> $3=<>> CWORD=3 WORDS=<mycmd,a,>,b>"),
    ])
    .await
}

/// Like bash, `COMP_WORDS` should keep a command or parameter substitution together as
/// one word, even though readline's word to complete (`$2`) doesn't.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO(completions): group $(...), ${...}, and `...` in COMP_WORDS"]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "the inputs are shell syntax"
)]
async fn completion_function_args_match_bash_in_substitutions() -> Result<()> {
    check_completion_function_args(&[
        (
            "mycmd $(a b|",
            "$2=<b> $3=<mycmd> CWORD=1 WORDS=<mycmd,$(a b>",
        ),
        (
            "mycmd $(a b)|",
            "$2=<b)> $3=<mycmd> CWORD=1 WORDS=<mycmd,$(a b)>",
        ),
        (
            "mycmd $(a b) c|",
            "$2=<c> $3=<$(a b)> CWORD=2 WORDS=<mycmd,$(a b),c>",
        ),
        (
            "mycmd x$(a b)y|",
            "$2=<b)y> $3=<mycmd> CWORD=1 WORDS=<mycmd,x$(a b)y>",
        ),
        (
            "mycmd ${a:b|",
            "$2=<b> $3=<mycmd> CWORD=1 WORDS=<mycmd,${a:b>",
        ),
        (
            "mycmd ${a:b}|",
            "$2=<b}> $3=<mycmd> CWORD=1 WORDS=<mycmd,${a:b}>",
        ),
        (
            "mycmd ${a:b} c|",
            "$2=<c> $3=<${a:b}> CWORD=2 WORDS=<mycmd,${a:b},c>",
        ),
        (
            "mycmd `a b|",
            "$2=<b> $3=<mycmd> CWORD=1 WORDS=<mycmd,`a b>",
        ),
    ])
    .await
}

/// Like bash, completion should only see the simple command containing the cursor: after
/// the last command separator, and past any leading variable assignments.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO(completions): scope completion to the current simple command "]
async fn completion_function_args_match_bash_in_command_sequences() -> Result<()> {
    check_completion_function_args(&[
        (
            "echo x; mycmd a|",
            "$2=<a> $3=<mycmd> CWORD=1 WORDS=<mycmd,a>",
        ),
        (
            "echo x && mycmd a|",
            "$2=<a> $3=<mycmd> CWORD=1 WORDS=<mycmd,a>",
        ),
        ("(mycmd a|", "$2=<a> $3=<mycmd> CWORD=1 WORDS=<mycmd,a>"),
        ("X=1 mycmd a|", "$2=<a> $3=<mycmd> CWORD=1 WORDS=<mycmd,a>"),
    ])
    .await
}

/// The cursor position is a byte offset, which must be a char boundary in the line.
#[tokio::test(flavor = "multi_thread")]
async fn completion_position_must_be_char_boundary() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    assert!(test_shell.complete_end_of_line_full("ls é").await.is_ok());
    // Inside the multi-byte `é`, or past the end of the line.
    assert!(test_shell.complete("ls é", 4).await.is_err());
    assert!(test_shell.complete("ls", 3).await.is_err());

    Ok(())
}

/// Tests the range completions replace when the word being completed is in an open quote:
/// the candidates replace that quote too.
#[tokio::test(flavor = "multi_thread")]
async fn completion_range_in_open_quote() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.run("complete -W 'ab' mycmd").await?;

    for (line, start) in [
        ("mycmd a", 6),
        ("mycmd 'a", 6),
        ("mycmd --x=\"a", 10),
        ("mycmd x'a", 7),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(replaced_range(&completions)?, start..line.len(), "{line}");
        assert_eq!(candidate_texts(&completions), ["ab"], "{line}");
    }

    Ok(())
}

/// Like bash, a completion command (`complete -C`) gets the `COMP_*` variables exported to
/// it, and they aren't left set afterwards. Running it doesn't change `$?`.
#[tokio::test(flavor = "multi_thread")]
async fn completion_command_gets_comp_vars() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        // The command exits 0 (its arguments go to `true`), unlike the `false` before it.
        .run("complete -C 'printenv COMP_LINE COMP_POINT; true' cmd; false")
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd x").await?;
    assert_eq!(candidate_texts(&completions), ["5", "cmd x"]);

    test_shell
        .run("status=$?; [[ -v COMP_LINE ]] && line=set")
        .await?;
    assert_eq!(test_shell.get_var("status").as_deref(), Some("1"));
    assert_eq!(test_shell.get_var("line"), None);

    Ok(())
}

/// Like bash, a completion function's `COMP_*` variables aren't left set afterwards, but
/// what it leaves in other variables is.
#[tokio::test(flavor = "multi_thread")]
async fn completion_function_vars_are_not_left_set() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        .run(r#"_f() { seen="$COMP_LINE|${COMP_WORDS[*]}"; COMPREPLY=(xy); }; complete -F _f cmd"#)
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd x").await?;
    assert_eq!(candidate_texts(&completions), ["xy"]);
    assert_eq!(test_shell.get_var("seen").as_deref(), Some("cmd x|cmd x"));

    for var in [
        "COMP_LINE",
        "COMP_POINT",
        "COMP_WORDS",
        "COMP_CWORD",
        "COMPREPLY",
    ] {
        assert_eq!(test_shell.get_var(var), None, "{var}");
    }

    Ok(())
}

/// Like bash, the empty-line spec (`complete -E`) applies when there's nothing before the
/// cursor and it isn't at the start of a word; otherwise, at the start of a line, the
/// initial-word spec (`complete -I`) does. Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn empty_line_spec_completes_empty_line() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run("complete -E -W empty; complete -I -W initial")
        .await?;

    for (line, cursor, expected) in [
        ("", 0, "empty"),
        ("  cmd", 0, "empty"),
        ("  ", 2, "initial"),
        ("cmd", 0, "initial"),
    ] {
        let completions = test_shell.complete(line, cursor).await?;
        assert_eq!(
            candidate_texts(&completions),
            [expected],
            "{line:?} at {cursor}"
        );
    }

    Ok(())
}

/// Completion gives up, with no candidates, if a completion function keeps asking for it to
/// restart.
#[tokio::test(flavor = "multi_thread")]
async fn completion_that_keeps_restarting_gives_up() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run("_restart() { return 124; }; complete -F _restart -D")
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd xy").await?;
    assert_eq!(candidate_texts(&completions), [] as [String; 0]);

    Ok(())
}

/// Like bash, a completion function for a special spec gets bash's name for it as `$1`;
/// with no words on the line, `COMP_WORDS` is empty and `COMP_CWORD` is -1; and when
/// completing the first word, `$3` is that word. Tests each input with the cursor at its
/// `|`; expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn special_spec_function_args_match_bash() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run(
            r#"
_test_comp() {
    local IFS=,
    CAPTURED="\$1=<$1> \$2=<$2> \$3=<$3> CWORD=$COMP_CWORD NWORDS=${#COMP_WORDS[@]} WORDS=<${COMP_WORDS[*]}>"
}
complete -E -F _test_comp
complete -I -F _test_comp
complete -D -F _test_comp
"#,
        )
        .await?;

    for (input, expected) in [
        (
            "|",
            "$1=<_EmptycmD_> $2=<> $3=<> CWORD=-1 NWORDS=0 WORDS=<>",
        ),
        (
            "|  cmd",
            "$1=<_EmptycmD_> $2=<> $3=<> CWORD=0 NWORDS=2 WORDS=<,cmd>",
        ),
        (
            "  |",
            "$1=<_InitialWorD_> $2=<> $3=<> CWORD=-1 NWORDS=0 WORDS=<>",
        ),
        (
            "cm|",
            "$1=<_InitialWorD_> $2=<cm> $3=<cm> CWORD=0 NWORDS=1 WORDS=<cm>",
        ),
        (
            "|cmd",
            "$1=<_InitialWorD_> $2=<> $3=<cmd> CWORD=0 NWORDS=1 WORDS=<cmd>",
        ),
        (
            "foo x|",
            "$1=<foo> $2=<x> $3=<foo> CWORD=1 NWORDS=2 WORDS=<foo,x>",
        ),
    ] {
        test_shell.set_var("CAPTURED", "")?;
        test_shell.complete_at_marker(input).await?;
        assert_eq!(
            test_shell.get_var("CAPTURED").as_deref(),
            Some(expected),
            "{input}"
        );
    }

    Ok(())
}

/// Like bash, the `file` action makes a completion's candidates file names, and so does the
/// `directory` action if it finds any. Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn file_and_directory_actions_complete_file_names() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("a b").touch()?;
    test_shell.temp_dir.child("dir x").create_dir_all()?;
    test_shell
        .run(
            r"complete -f filecmd; complete -d dircmd
              complete -W 'q\ r' -f wfilecmd; complete -W 'q\ r' -d wdircmd",
        )
        .await?;

    for (line, expected, file_names) in [
        ("filecmd a", "a b", true),
        ("dircmd di", "dir x", true),
        // Even with no file names found, `-f` makes the other candidates file names...
        ("wfilecmd q", "q r", true),
        // ...but `-d` doesn't, if it finds no directories.
        ("wdircmd q", "q r", false),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), [expected], "{line}");
        assert_eq!(completions.options.treat_as_filenames, file_names, "{line}");
    }

    Ok(())
}

/// File names that `-o default` or `-o dirnames` falls back to are file names, quoted as
/// such, whether or not directories are to be marked. Checked with bash 5.3, which quotes
/// them with `mark-directories` off.
#[tokio::test(flavor = "multi_thread")]
async fn default_fallback_file_names_are_quoted_without_marked_directories() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("a b").touch()?;
    test_shell.temp_dir.child("sub dir").create_dir_all()?;
    test_shell
        .run("complete -o default mycmd; complete -o dirnames -W '' dircmd")
        .await?;
    test_shell
        .shell
        .completion_config_mut()
        .fallback_options
        .mark_directories = false;

    for (line, expected) in [("mycmd a", "a b"), ("dircmd su", "sub dir")] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), [expected], "{line}");
        assert!(completions.options.treat_as_filenames, "{line}");
    }

    Ok(())
}

/// Like bash, `compopt` changes the options of the completion in progress, which a
/// `compgen` call from the completion function doesn't disturb.
#[tokio::test(flavor = "multi_thread")]
async fn compopt_survives_compgen_in_completion_function() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        .run(
            "_f() { compopt -o nospace; compgen -W x >/dev/null; COMPREPLY=(xy); }; complete -F _f cmd",
        )
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd x").await?;
    assert_eq!(completions.candidates, ["xy"]);
    assert!(completions.options.no_trailing_space_at_end_of_line);

    Ok(())
}

/// Like bash, a subshell of a completion function is in the completion too (so `compopt`
/// succeeds there), but `compopt` there changes only the subshell's options, not the
/// completion's.
#[tokio::test(flavor = "multi_thread")]
async fn compopt_in_subshell_does_not_change_completion() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        .run("_f() { (compopt -o nospace) && COMPREPLY=(xy); }; complete -F _f cmd")
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd x").await?;
    assert_eq!(completions.candidates, ["xy"]);
    assert!(!completions.options.no_trailing_space_at_end_of_line);

    Ok(())
}

/// A completion that's cancelled (e.g. by Ctrl-C) while its completion function runs leaves
/// nothing of it behind: the function's call, its variables (and the `COMP_*` ones set for
/// it), the block on traps, or the completion in progress (so `compopt` acts as it does
/// outside one).
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_completion_leaves_nothing_behind() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run("_slow() { local x=1; FOO=1 _block; }; _block() { sleep 10 >/dev/null 2>&1; }; complete -F _slow mycmd")
        .await?;

    let completion = test_shell.complete_end_of_line_full("mycmd x");
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), completion).await;
    assert!(result.is_err(), "the completion should have been cancelled");

    let shell = &test_shell.shell;
    assert!(!shell.in_function());
    assert!(shell.env_str("x").is_none());
    assert!(shell.env_str("FOO").is_none());

    test_shell.run("compopt -o nospace; rc=$?").await?;
    assert_eq!(test_shell.get_var("rc").as_deref(), Some("1"));

    test_shell
        .run("leaked=0; [[ -v COMP_LINE ]] && leaked=1")
        .await?;
    assert_eq!(test_shell.get_var("leaked").as_deref(), Some("0"));
    assert!(!test_shell.shell.call_stack().is_trap_delivery_suppressed());

    Ok(())
}

/// Tests native variable completion without braces (e.g., $VAR)
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_variable_names() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    // Set test variables.
    test_shell.set_var("TESTVAR1", "value1")?;
    test_shell.set_var("TESTVAR2", "value2")?;

    // Complete.
    let completions = test_shell
        .complete_end_of_line_full("echo $TESTVAR")
        .await?;
    let results: Vec<String> = completions.candidates.into_iter().collect();
    assert_eq!(results, ["$TESTVAR1", "$TESTVAR2"]);

    // Variable completions should not be treated as filenames (to avoid escaping $)
    assert!(
        !completions.options.treat_as_filenames,
        "variable completions should not be treated as filenames"
    );

    Ok(())
}

/// Tests native variable completion with braces (e.g., ${VAR})
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_variable_names_with_braces() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    // Set test variables.
    test_shell.set_var("TESTVAR1", "value1")?;
    test_shell.set_var("TESTVAR2", "value2")?;

    // Complete.
    let completions = test_shell
        .complete_end_of_line_full("echo ${TESTVAR")
        .await?;
    let results: Vec<String> = completions.candidates.into_iter().collect();
    assert_eq!(results, ["${TESTVAR1}", "${TESTVAR2}"]);

    // Variable completions should not be treated as filenames (to avoid escaping $)
    assert!(
        !completions.options.treat_as_filenames,
        "variable completions should not be treated as filenames"
    );

    Ok(())
}

/// Tests that file completion works after a variable (e.g., $VAR/path)
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_path_after_variable() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    // Create files in temp dir.
    test_shell.temp_dir.child("file1.txt").touch()?;
    test_shell.temp_dir.child("file2.txt").touch()?;

    // Set a variable pointing to the temp dir.
    let temp_path = test_shell.temp_dir.path().to_str().unwrap().to_owned();
    test_shell.set_var("MYDIR", &temp_path)?;

    // Complete files after variable expansion.
    // The completion system expands variables before completing, so results use expanded paths.
    let completions = test_shell
        .complete_end_of_line_full("ls $MYDIR/file")
        .await?;
    let results: Vec<String> = completions.candidates.into_iter().collect();
    let expected = [
        std::format!("{temp_path}/file1.txt"),
        std::format!("{temp_path}/file2.txt"),
    ];
    assert_eq!(results, expected);

    // Path completions should be treated as filenames
    assert!(
        completions.options.treat_as_filenames,
        "path completions should be treated as filenames"
    );

    Ok(())
}

/// Like bash, `compopt -o nosort` in a completion function keeps the candidates in the order
/// generated.
#[tokio::test(flavor = "multi_thread")]
async fn compopt_nosort_applies_to_completion_in_progress() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        .run("_f() { compopt -o nosort; COMPREPLY=(xb xa xc); }; complete -F _f cmd")
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd x").await?;
    assert_eq!(completions.candidates, ["xb", "xa", "xc"]);

    Ok(())
}

/// A `compgen` call in a completion function doesn't leak its options (e.g. its own
/// `nosort`) into the completion: like bash, the candidates are still sorted.
#[tokio::test(flavor = "multi_thread")]
async fn compgen_in_completion_function_keeps_sorting() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        .run("_f() { compgen -W x >/dev/null; COMPREPLY=(xb xa); }; complete -F _f cmd")
        .await?;

    let completions = test_shell.complete_end_of_line_full("cmd x").await?;
    assert_eq!(completions.candidates, ["xa", "xb"]);

    Ok(())
}

/// Like bash, a `complete -X` pattern's leading `!` is interpreted when completing, not
/// when the spec is registered: with extglob on by then, `!(...)` is an extglob pattern.
#[tokio::test(flavor = "multi_thread")]
async fn completion_filter_is_interpreted_when_completing() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    let exec_params = test_shell.shell.default_exec_params();
    test_shell
        .shell
        .run_string(
            "shopt -u extglob; complete -W 'foo bar fab' -X '!(f*)' mycmd; shopt -s extglob"
                .to_owned(),
            &brush_core::SourceInfo::default(),
            &exec_params,
        )
        .await?;

    // `!(f*)` matches what doesn't start with `f`, so those are removed.
    let completions = test_shell.complete_end_of_line_full("mycmd ").await?;
    assert_eq!(completions.candidates, ["fab", "foo"]);

    Ok(())
}
