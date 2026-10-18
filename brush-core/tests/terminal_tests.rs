//! The shell moves the terminal's foreground through the terminal its host names.

#![cfg(test)]
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use brush_core::extensions::{
    DefaultErrorFormatter, DefaultProcessLauncher, ShellExtensionsImpl, Terminal,
};

/// A terminal that counts what the shell asks of it.
#[derive(Clone, Default)]
struct Counting {
    gives: Arc<AtomicUsize>,
    takes: Arc<AtomicUsize>,
}

impl Terminal for Counting {
    fn give_foreground(&self, _group: brush_core::sys::process::ProcessId) -> std::io::Result<()> {
        self.gives.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn take_foreground(&self) -> std::io::Result<()> {
        self.takes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, DefaultProcessLauncher, Counting>;

/// An interactive shell takes the foreground back once a program it ran in the foreground
/// ends, on the terminal its host named.
#[tokio::test]
async fn an_interactive_shell_takes_the_foreground_back_after_a_program() -> Result<()> {
    let terminal = Counting::default();
    let mut shell = brush_core::Shell::builder_with_extensions::<Extensions>()
        .profile(brush_core::ProfileLoadBehavior::Skip)
        .rc(brush_core::RcLoadBehavior::Skip)
        .interactive(true)
        .terminal(terminal.clone())
        .build()
        .await?;

    let params = shell.default_exec_params();
    shell
        .run_string(
            "sh -c 'exit 0'",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;

    assert_eq!(terminal.takes.load(Ordering::SeqCst), 1);
    assert_eq!(terminal.gives.load(Ordering::SeqCst), 0);
    Ok(())
}
