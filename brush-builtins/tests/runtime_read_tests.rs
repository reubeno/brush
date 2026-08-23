//! Regression tests for synchronous builtin reads in embedded Tokio runtimes.

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::expect_used)]

use std::time::Duration;

use brush_builtins::ShellBuilderExt as _;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const CHILD_TEST_ENV: &str = "BRUSH_RUNTIME_READ_TEST_CHILD";

async fn run_script(script: &str) {
    let mut shell = brush_core::Shell::builder()
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .build()
        .await
        .expect("shell should build");
    let params = shell.default_exec_params();
    let result = shell
        .run_string(script, &brush_core::SourceInfo::default(), &params)
        .await
        .expect("script should execute");

    assert!(
        result.is_success(),
        "script failed with exit code {}",
        u8::from(result.exit_code)
    );
}

fn run_on_multi_thread_runtime(script: &str) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multi-thread runtime should build");
    let local_set = tokio::task::LocalSet::new();

    runtime.block_on(local_set.run_until(async {
        tokio::time::timeout(TEST_TIMEOUT, run_script(script))
            .await
            .expect("script deadlocked");
    }));
}

fn run_on_current_thread_runtime(script: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime should build");
    let local_set = tokio::task::LocalSet::new();

    runtime.block_on(local_set.run_until(async {
        tokio::time::timeout(TEST_TIMEOUT, run_script(script))
            .await
            .expect("script deadlocked");
    }));
}

fn run_current_thread_test_in_subprocess(
    test_name: &str,
    script: &str,
) -> Result<(), std::io::Error> {
    if std::env::var_os(CHILD_TEST_ENV).is_some_and(|value| value == test_name) {
        run_on_current_thread_runtime(script);
        return Ok(());
    }

    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_TEST_ENV, test_name)
        .spawn()?;
    let deadline = std::time::Instant::now() + TEST_TIMEOUT;

    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(
                    "runtime regression subprocess failed",
                ))
            };
        }

        if std::time::Instant::now() >= deadline {
            child.kill()?;
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "runtime regression subprocess deadlocked",
            ));
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn read_process_substitution_completes_in_local_set_on_multi_thread_runtime() {
    run_on_multi_thread_runtime(
        r#"
result=$(
    while IFS= read -r line; do
        printf '<%s>' "$line"
    done < <(printf 'one\ntwo\n')
)
[[ $result == '<one><two>' ]]
"#,
    );
}

#[test]
fn mapfile_process_substitution_completes_in_local_set_on_multi_thread_runtime() {
    run_on_multi_thread_runtime(
        r#"
result=$(
    mapfile -t lines < <(printf 'one\ntwo\n')
    printf '%s' "${lines[*]}"
)
[[ $result == 'one two' ]]
"#,
    );
}

#[test]
fn read_does_not_use_block_in_place_on_current_thread_runtime() {
    run_current_thread_test_in_subprocess(
        "read_does_not_use_block_in_place_on_current_thread_runtime",
        r#"
result=$(
    while IFS= read -r line; do
        printf '<%s>' "$line"
    done < <(printf 'one\ntwo\n')
)
[[ $result == '<one><two>' ]]
"#,
    )
    .expect("current-thread read scenario should complete");
}

#[test]
fn mapfile_does_not_use_block_in_place_on_current_thread_runtime() {
    run_current_thread_test_in_subprocess(
        "mapfile_does_not_use_block_in_place_on_current_thread_runtime",
        r#"
result=$(
    mapfile -t lines < <(printf 'one\ntwo\n')
    printf '%s' "${lines[*]}"
)
[[ $result == 'one two' ]]
"#,
    )
    .expect("current-thread mapfile scenario should complete");
}
