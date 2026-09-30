//! Completion integration tests for brush shell.

// For now, only compile this for Linux.
#![cfg(target_os = "linux")]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use assert_fs::prelude::*;
use brush_builtins::ShellBuilderExt;
use std::path::PathBuf;

/// Returns the value of each candidate in `completions`.
fn candidate_texts(completions: &brush_core::completion::Completions) -> Vec<String> {
    completions
        .candidates
        .iter()
        .map(|candidate| candidate.value.clone())
        .collect()
}

/// Returns the text each candidate in `completions` edits the line with.
fn candidate_edit_texts(completions: &brush_core::completion::Completions) -> Vec<String> {
    completions
        .candidates
        .iter()
        .map(|candidate| candidate.edit.text.clone())
        .collect()
}

/// Returns the range of the line that the only candidate in `completions` replaces.
fn replaced_range(
    completions: &brush_core::completion::Completions,
) -> Result<std::ops::Range<usize>> {
    match completions.candidates.as_slice() {
        [candidate] => Ok(candidate.edit.replace.clone()),
        candidates => Err(anyhow::anyhow!(
            "expected one candidate, got {candidates:?}"
        )),
    }
}

/// Returns whether `completions` has candidates, all of them file names.
fn are_file_names(completions: &brush_core::completion::Completions) -> bool {
    !completions.candidates.is_empty()
        && completions.candidates.iter().all(|candidate| {
            matches!(
                candidate.kind,
                brush_core::completion::CandidateKind::FileName { .. }
            )
        })
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
        let prefs = self.shell.completion_config().edit_prefs.clone();
        Ok(self.shell.complete(line, pos, &prefs).await?)
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

/// Like bash (with bash-completion), file names under a `$VAR` directory keep it as typed.
/// Expected values were captured from bash 5.3.
#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_with_var() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls $PWD/item").await?;
    assert_eq!(results, ["$PWD/item1", "$PWD/item2"]);

    Ok(())
}

/// Like bash (with bash-completion), file names under `~` keep it as typed. Expected values
/// were captured from bash 5.3.
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
    assert_eq!(results, ["~/item1", "~/item2"]);

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

    // Use upstream bash-completion's spec for `ls`; some distros (e.g. openSUSE) add options.
    test_shell
        .run("complete -F _comp_complete_longopt ls")
        .await?;

    test_shell.temp_dir.child("item1 item2").touch()?;
    test_shell.temp_dir.child("item1'item2").touch()?;

    let mut results = test_shell.complete_end_of_line("ls item1\\ ").await?;
    assert_eq!(results, ["item1 item2"]);

    // Like bash, the `'` opens a quote, so this completes an empty word in it, for which
    // bash-completion offers nothing (see native_complete_empty_word_in_open_quote for
    // brush's own completion).
    results = test_shell.complete_end_of_line("ls item1'").await?;
    assert_eq!(results, Vec::<String>::new());

    results = test_shell.complete_end_of_line("ls item1").await?;
    assert_eq!(results, ["item1 item2", "item1'item2"]);

    results = test_shell.complete_end_of_line("ls 'item1 ").await?;
    assert_eq!(results, ["item1 item2"]);

    results = test_shell.complete_end_of_line("ls \"item1 ").await?;
    assert_eq!(results, ["item1 item2"]);

    // With `-o default` (as openSUSE registers `ls`), the empty word in the open quote
    // falls back to file names.
    test_shell
        .run("complete -o default -F _comp_complete_longopt ls")
        .await?;
    results = test_shell.complete_end_of_line("ls item1'").await?;
    assert_eq!(results, ["item1 item2", "item1'item2"]);

    Ok(())
}

/// Tests that interactive completion sets `COMP_KEY` and `COMP_TYPE` to 9 (TAB).
#[tokio::test(flavor = "multi_thread")]
async fn interactive_completion_sets_comp_key_and_comp_type() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run(
            r"
_test_comp() {
    CAPTURED_COMP_KEY=$COMP_KEY
    CAPTURED_COMP_TYPE=$COMP_TYPE
    COMPREPLY=(done)
}
complete -F _test_comp mycmd
",
        )
        .await?;

    test_shell.complete_end_of_line_full("mycmd ").await?;

    assert_eq!(
        test_shell.get_var("CAPTURED_COMP_KEY").as_deref(),
        Some("9"),
        "COMP_KEY should be 9 (TAB)"
    );
    assert_eq!(
        test_shell.get_var("CAPTURED_COMP_TYPE").as_deref(),
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

/// Tests the range completions replace, and the text they replace it with, when the word
/// being completed is in an open quote: the candidates, once quoted, replace that quote too.
#[tokio::test(flavor = "multi_thread")]
async fn completion_range_in_open_quote() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.run("complete -W 'ab' mycmd").await?;

    for (line, start, text) in [
        ("mycmd a", 6, "ab "),
        ("mycmd 'a", 6, "'ab' "),
        ("mycmd --x=\"a", 10, "\"ab\" "),
        ("mycmd x'a", 7, "'ab' "),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(replaced_range(&completions)?, start..line.len(), "{line}");
        assert_eq!(candidate_edit_texts(&completions), [text], "{line}");
        assert_eq!(candidate_texts(&completions), ["ab"], "{line}");
    }

    Ok(())
}

/// Tests `compgen -f` dequoting from a completion function, like bash: the word being
/// completed is dequoted once, and a word the function quoted itself (as
/// bash-completion does with `printf %q`) is dequoted once more.
#[tokio::test(flavor = "multi_thread")]
async fn compgen_f_dequotes_in_completion_function() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("a'b").child("c").touch()?;
    test_shell.temp_dir.child("a$b").child("d").touch()?;
    test_shell.temp_dir.child("a\"b").child("e").touch()?;
    test_shell.temp_dir.child("a\nb").touch()?;

    test_shell
        .run(
            r#"
_as_is() { COMPREPLY=($(compgen -f -- "$2")); }
_quoted() { local q; printf -v q %q "$2"; COMPREPLY=($(compgen -f -- "$q")); }
_unquoted() { local w=${COMP_WORDS[COMP_CWORD]}; COMPREPLY=($(compgen -f -- "${w#\'}")); }
complete -F _as_is as_is
complete -F _quoted quoted
complete -F _unquoted unquoted
_nl() { COMPREPLY=("$(compgen -f -- $'"a\\\nb')"); }
complete -F _nl nl
"#,
        )
        .await?;

    for (line, expected) in [
        // In an open quote, `$2` excludes the quote and file names are generated as if
        // inside it.
        (r#"as_is 'a"b/"#, r#"a"b/e"#),
        (r#"unquoted 'a"b/"#, r#"a"b/e"#),
        (r"as_is a\'b/", "a'b/c"),
        (r"quoted a\'b/", "a'b/c"),
        (r"as_is a\$b/", "a$b/d"),
        (r"quoted a\$b/", "a$b/d"),
        (r#"as_is "a\"b/"#, r#"a"b/e"#),
        (r#"quoted "a\"b/"#, r#"a"b/e"#),
        // Like bash, a backslash before a newline in double
        // quotes is removed, but the newline is kept.
        (r#"nl "a"#, "a\nb"),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), [expected], "{line}");
    }

    // Like bash, a word the function quoted itself is dequoted in the open quote, where
    // backslashes are literal, so this looks for `a\"b/` and finds nothing.
    let completions = test_shell
        .complete_end_of_line_full(r#"quoted 'a"b/"#)
        .await?;
    assert_eq!(completions.candidates.len(), 0);

    Ok(())
}

/// Like bash, a word a completion function passes to `compgen` (rather than `$2`) is
/// dequoted a second time only if the line has quoting before the cursor, in the word
/// being completed or an earlier one. Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn compgen_f_dequotes_again_only_if_quoting_before_cursor() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child(r"a\b").touch()?;
    test_shell.temp_dir.child("ab").touch()?;

    test_shell
        .run(r"_f() { COMPREPLY=($(compgen -f -- 'a\\b')); }; complete -F _f zz")
        .await?;

    for (line, expected) in [
        ("zz x", &[r"a\b"][..]),
        (r"zz \x", &["ab"]),
        ("zz 'q' x", &["ab"]),
        // Backslashes are literal in single quotes, so neither dequoting removes them.
        ("zz 'x", &[]),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), expected, "{line}");
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
    assert_eq!(candidate_texts(&completions), ["xy"]);
    // With `nospace`, no space follows.
    assert_eq!(candidate_edit_texts(&completions), ["xy"]);

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
    assert_eq!(candidate_texts(&completions), ["xy"]);
    // Without `nospace`, a space follows.
    assert_eq!(candidate_edit_texts(&completions), ["xy "]);

    Ok(())
}

/// Like bash, a completion command (`complete -C`) gets the `COMP_*` variables exported to
/// it, and they aren't left set afterwards. Running it doesn't change `$?`.
#[tokio::test(flavor = "multi_thread")]
async fn completion_command_gets_comp_vars() -> Result<()> {
    let mut test_shell = TestShell::new().await?;

    test_shell
        .run("complete -C 'printenv COMP_LINE COMP_POINT' cmd; false")
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

/// Like bash, the file name being completed is dequoted but not expanded, so a `$` or a
/// backslash in it is matched literally. Expected values were captured from bash 5.3.
#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_file_name_with_dollar_and_backslash() -> Result<()> {
    let mut test_shell = TestShell::with_bash_completion().await?;
    test_shell.temp_dir.child("a$bc").touch()?;
    test_shell.temp_dir.child(r"a\bq").touch()?;
    test_shell.temp_dir.child("axy").touch()?;

    for (line, expected) in [
        ("ls a$b", &["a$bc"][..]),
        ("ls 'x' a$b", &["a$bc"]),
        (r"cat a\\b", &[r"a\bq"]),
    ] {
        assert_eq!(
            test_shell.complete_end_of_line(line).await?,
            expected,
            "{line}"
        );
    }

    Ok(())
}

// Tests for native completion fallback (without bash-completion installed)

/// Like bash, the file name being completed is dequoted (in the quote it's in) but not
/// expanded. Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_file_name_is_only_dequoted() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("a$bc").touch()?;
    test_shell.temp_dir.child(r"a\bq").touch()?;
    test_shell.temp_dir.child("axy").touch()?;

    for (line, expected) in [
        ("ls a$b", &["a$bc"][..]),
        (r"ls a\$b", &["a$bc"]),
        // A backslash in double quotes escapes only a few chars, so this one is literal.
        (r#"ls "a\"#, &[r"a\bq"]),
        (r#"ls "a\b"#, &[r"a\bq"]),
        (r"ls 'a\", &[r"a\bq"]),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), expected, "{line}");
    }

    Ok(())
}

/// Like bash, the `'` in `item1'` opens a quote, so this completes the empty word in it,
/// to every file in the directory.
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_empty_word_in_open_quote() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("item1 item2").touch()?;
    test_shell.temp_dir.child("item1'item2").touch()?;

    let completions = test_shell.complete_end_of_line_full("ls item1'").await?;
    assert_eq!(
        candidate_texts(&completions),
        ["item1 item2", "item1'item2"]
    );

    Ok(())
}

/// Like bash's default completion, a glob pattern that matches no file name as a prefix
/// completes to the one file name it matches, if it matches exactly one. Expected values
/// were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_glob_pattern() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("foo.txt").touch()?;
    test_shell.temp_dir.child("fob").touch()?;
    test_shell.temp_dir.child("sub").child("x").touch()?;

    for (line, expected) in [
        ("ls *.txt", &["foo.txt"][..]),
        ("ls f*.txt", &["foo.txt"]),
        ("ls fob*", &["fob"]),
        ("ls s*", &["sub"]),
        // bash globs the word even in an open quote.
        ("ls \"s*", &["sub"]),
        // The pattern must match the whole file name, and just one.
        ("ls *.t", &[]),
        ("ls fo*", &[]),
        // Quoted glob chars don't glob; bash matches quote chars in the word literally.
        (r"ls \*.txt", &[]),
        ("ls \"*\".txt", &[]),
        ("ls 'f'*.txt", &[]),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), expected, "{line}");
    }

    Ok(())
}

/// Like bash, a glob pattern starting with `~` is tilde-expanded before it's matched.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO(completions): tilde-expand glob patterns in default completion"]
async fn native_complete_glob_pattern_with_tilde() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("foo.txt").touch()?;
    let home = test_shell.temp_dir.path().to_string_lossy().into_owned();
    test_shell.set_var("HOME", &home)?;

    let completions = test_shell.complete_end_of_line_full("ls ~/f*.txt").await?;
    assert_eq!(candidate_texts(&completions), [format!("{home}/foo.txt")]);

    Ok(())
}

/// Like bash, `complete -o bashdefault` falls back to bash's default completions (which
/// include completing a glob pattern, but not file names) when a spec yields nothing.
/// Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO(completions): implement complete -o bashdefault"]
async fn complete_bashdefault_falls_back_to_default_completion() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("foo.txt").touch()?;

    test_shell
        .run("_none() { :; }; complete -o bashdefault -F _none mycmd")
        .await?;

    for (line, expected) in [("mycmd *.txt", &["foo.txt"][..]), ("mycmd fo", &[])] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), expected, "{line}");
    }

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
    let results = candidate_texts(&completions);
    assert_eq!(results, ["$TESTVAR1", "$TESTVAR2"]);

    // Variable completions should not be treated as filenames (to avoid escaping $)
    assert!(
        !are_file_names(&completions),
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
    let results = candidate_texts(&completions);
    assert_eq!(results, ["${TESTVAR1}", "${TESTVAR2}"]);

    // Variable completions should not be treated as filenames (to avoid escaping $)
    assert!(
        !are_file_names(&completions),
        "variable completions should not be treated as filenames"
    );

    Ok(())
}

/// File names that `-o default` falls back to are file names, quoted as such, whether or
/// not directories are to be marked.
#[tokio::test(flavor = "multi_thread")]
async fn default_fallback_file_names_are_quoted_without_marked_directories() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("a b").touch()?;
    test_shell.run("complete -o default mycmd").await?;
    test_shell
        .shell
        .completion_config_mut()
        .edit_prefs
        .mark_directories = false;

    let completions = test_shell.complete_end_of_line_full("mycmd a").await?;
    assert_eq!(candidate_edit_texts(&completions), [r"a\ b "]);

    Ok(())
}

/// Like bash, `-o noquote` only stops file names being quoted: they're still file names, so
/// a directory is still marked with a trailing slash.
#[tokio::test(flavor = "multi_thread")]
async fn noquote_file_names_are_still_file_names() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell.temp_dir.child("sub").create_dir_all()?;
    test_shell.temp_dir.child("a b").touch()?;
    test_shell
        .run("complete -o filenames -o noquote -W sub dircmd; complete -o noquote -f filecmd")
        .await?;

    let completions = test_shell.complete_end_of_line_full("dircmd s").await?;
    assert_eq!(candidate_edit_texts(&completions), ["sub/"]);

    let completions = test_shell.complete_end_of_line_full("filecmd a").await?;
    assert_eq!(candidate_edit_texts(&completions), ["a b "]);

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

    for (line, expected) in [
        ("filecmd a", r"a\ b "),
        ("dircmd di", r"dir\ x/"),
        // Even with no file names found, `-f` makes the other candidates file names...
        ("wfilecmd q", r"q\ r "),
        // ...but `-d` doesn't, if it finds no directories.
        ("wdircmd q", "q r "),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_edit_texts(&completions), [expected], "{line}");
    }

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

    // Complete files under the variable's directory. Like bash, they show the variable as
    // typed, and are quoted so it still expands.
    let completions = test_shell
        .complete_end_of_line_full("ls $MYDIR/file")
        .await?;
    assert_eq!(
        candidate_texts(&completions),
        ["$MYDIR/file1.txt", "$MYDIR/file2.txt"]
    );
    assert_eq!(
        candidate_edit_texts(&completions),
        ["$MYDIR/file1.txt ", "$MYDIR/file2.txt "]
    );

    // Path completions should be treated as filenames
    assert!(
        are_file_names(&completions),
        "path completions should be treated as filenames"
    );

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

/// A completion that's cancelled (e.g. by Ctrl-C) while its completion function runs
/// leaves nothing behind: it's no longer in progress, so `compopt` and `compgen` act as
/// they do outside one; the `COMP_*` variables are unset; and traps aren't blocked.
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_completion_is_no_longer_in_progress() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run("_slow() { sleep 10 >/dev/null 2>&1; }; complete -F _slow mycmd")
        .await?;

    let completion = test_shell.complete("mycmd 'x", "mycmd 'x".len());
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), completion).await;
    assert!(result.is_err(), "the completion should have been cancelled");

    test_shell.run("compopt -o nospace; rc=$?").await?;
    assert_eq!(test_shell.get_var("rc").as_deref(), Some("1"));

    test_shell
        .run("leaked=0; [[ -v COMP_LINE ]] && leaked=1")
        .await?;
    assert_eq!(test_shell.get_var("leaked").as_deref(), Some("0"));
    assert!(!test_shell.shell.call_stack().is_trap_delivery_suppressed());

    Ok(())
}

/// Like bash, a completion cancelled (e.g. by Ctrl-C) while its completion function runs
/// leaves the shell out of that function: its call frame and local variables are gone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO(completions): interrupt a completion function on Ctrl-C rather than dropping it mid-call"]
async fn cancelled_completion_leaves_completion_function() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    test_shell
        .run("_slow() { local x=1; sleep 10 >/dev/null 2>&1; }; complete -F _slow mycmd")
        .await?;

    let completion = test_shell.complete("mycmd x", "mycmd x".len());
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), completion).await;
    assert!(result.is_err(), "the completion should have been cancelled");

    test_shell
        .run("depth=${#FUNCNAME[@]}; x=${x-unset}")
        .await?;
    assert_eq!(test_shell.get_var("depth").as_deref(), Some("0"));
    assert_eq!(test_shell.get_var("x").as_deref(), Some("unset"));

    Ok(())
}

/// Like bash, the directory part of a file name being completed in an open quote is
/// dequoted, then tilde- and parameter-expanded (even in single quotes) to find file names,
/// but not otherwise expanded; the candidates show it as typed. Expected values were
/// captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_expands_directory_in_open_quote() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    let home = test_shell.temp_dir.child("h");
    home.child("Docs dir").create_dir_all()?;
    let home = home.path().to_string_lossy().into_owned();
    test_shell.set_var("HOME", &home)?;
    test_shell.temp_dir.child("a b").child("file").touch()?;
    test_shell.temp_dir.child("x*y").child("file").touch()?;
    test_shell.temp_dir.child("q\"d").child("file").touch()?;

    // Each case's candidate, whether it's a directory, and how it's quoted: like bash, so
    // it names the file the candidate does, in the quote it's in.
    for (line, text, is_dir, quoted) in [
        (
            "echo \"~/Do",
            "~/Docs dir",
            true,
            format!("\"{home}/Docs dir\""),
        ),
        (
            "echo '~/Do",
            "~/Docs dir",
            true,
            format!("'{home}/Docs dir'"),
        ),
        (
            "echo \"$HOME/Do",
            "$HOME/Docs dir",
            true,
            "\"$HOME/Docs dir\"".to_owned(),
        ),
        (
            "echo '$HOME/Do",
            "$HOME/Docs dir",
            true,
            "'$HOME/Docs dir'".to_owned(),
        ),
        (
            "echo \"a b/fi",
            "a b/file",
            false,
            "\"a b/file\"".to_owned(),
        ),
        ("echo 'x*y/fi", "x*y/file", false, "'x*y/file'".to_owned()),
        (
            "echo 'q\"d/fi",
            "q\"d/file",
            false,
            "'q\"d/file'".to_owned(),
        ),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        let [candidate] = completions.candidates.as_slice() else {
            anyhow::bail!(
                "{line}: expected one candidate, got {:?}",
                completions.candidates
            );
        };
        assert_eq!(candidate.value, text, "{line}");
        assert_eq!(
            candidate.kind,
            brush_core::completion::CandidateKind::FileName { is_dir },
            "{line}"
        );
        // A directory is marked, and anything else followed by a space.
        let suffix = if is_dir { "/" } else { " " };
        assert_eq!(candidate.edit.text, quoted + suffix, "{line}");
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
    assert!(completions.candidates.is_empty());

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

/// Like bash, file names completed under a `~` or `$VAR` directory keep the directory as
/// typed, but `shopt -s direxpand` expands its parameters (not a leading `~`). Expected
/// values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_keeps_directory_as_typed() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    let home = test_shell.temp_dir.child("h");
    home.child("Docs dir").create_dir_all()?;
    home.child("plainfile").touch()?;
    let home = home.path().to_string_lossy().into_owned();
    test_shell.set_var("HOME", &home)?;

    for (line, expected) in [
        ("echo ~/Do", "~/Docs dir"),
        ("echo $HOME/Do", "$HOME/Docs dir"),
        ("echo ${HOME}/pl", "${HOME}/plainfile"),
        ("echo \"$HOME/Do", "$HOME/Docs dir"),
        ("echo '$HOME/Do", "$HOME/Docs dir"),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(candidate_texts(&completions), [expected], "{line}");
    }

    test_shell.run("shopt -s direxpand").await?;
    let docs = format!("{home}/Docs dir");
    for (line, expected) in [
        ("echo $HOME/Do", docs.as_str()),
        ("echo ~/Do", "~/Docs dir"),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        assert_eq!(
            candidate_texts(&completions),
            [expected],
            "direxpand: {line}"
        );
    }

    Ok(())
}

/// Like readline, several candidates starting with `~/` complete to their common prefix
/// with the `~` kept as typed, in a quote too (where a single candidate's is expanded). With
/// nothing past the `~/` to add, that leaves the line as is, rather than quoting it to
/// `~''/`. Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
async fn native_complete_common_prefix_keeps_tilde() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    let home = test_shell.temp_dir.child("h");
    home.child("pre fix1").touch()?;
    home.child("pre fix2").touch()?;
    home.child("plain1").touch()?;
    home.child("plain2").touch()?;
    home.child("zed").touch()?;
    let home = home.path().to_string_lossy().into_owned();
    test_shell.set_var("HOME", &home)?;

    for (line, expected) in [
        ("ls ~/pre", r"~/pre\ fix"),
        ("ls \"~/pre", "\"~/pre fix"),
        ("ls '~/pre", "'~/pre fix"),
        ("ls \"~/pl", "\"~/plain"),
    ] {
        let completions = test_shell.complete_end_of_line_full(line).await?;
        let prefix = completions.common_prefix.map(|prefix| prefix.text);
        assert_eq!(prefix.as_deref(), Some(expected), "{line}");
    }

    let completions = test_shell.complete_end_of_line_full("ls ~/").await?;
    assert_eq!(completions.common_prefix, None);

    Ok(())
}

/// Like bash, readline's `expand-tilde` variable expands a leading `~` in file names being
/// completed. Expected values were captured from bash 5.3.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO(completions): support readline variables set with `bind`, e.g. expand-tilde"]
async fn native_complete_with_expand_tilde() -> Result<()> {
    let mut test_shell = TestShell::new().await?;
    let home = test_shell.temp_dir.child("h");
    home.child("plainfile").touch()?;
    let home = home.path().to_string_lossy().into_owned();
    test_shell.set_var("HOME", &home)?;

    test_shell.run("bind 'set expand-tilde on'").await?;
    let completions = test_shell.complete_end_of_line_full("echo ~/pl").await?;
    assert_eq!(candidate_texts(&completions), [format!("{home}/plainfile")]);

    Ok(())
}
