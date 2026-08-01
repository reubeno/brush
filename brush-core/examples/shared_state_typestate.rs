//! Example: registering builtins that share typed state, verifying the
//! `SharedBuilder` / `SharedHandle` API accepts factory-fresh registrations in
//! any local-state typestate.
//!
//! ```bash
//! cargo run --package brush-core --example shared_state_typestate
//! ```

use brush_core::builtins::{self, SharedBuilder};
use brush_core::extensions::DefaultShellExtensions;
use brush_core::{ExecutionContext, ExecutionResult};

#[derive(Clone, Default)]
#[allow(dead_code)]
struct Counter(usize);

/// A trivial builtin whose `SharedState` is `Counter`.
#[derive(Default, clap::Parser)]
struct TempCommand;

impl builtins::Command for TempCommand {
    type State = ();
    type SharedState = Counter;
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::extensions::ShellExtensions>(
        &self,
        _ctx: ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shell = brush_core::Shell::builder().build().await?;

    // 1. SharedBuilder accepts a factory-fresh registration (NeedsLocalState).
    let builder = SharedBuilder::new(Counter::default()).builtin(
        "temp",
        builtins::builtin::<TempCommand, DefaultShellExtensions>(),
    );
    shell.register_shared(builder);

    // 2. SharedHandle accepts a with_state'd registration (HasLocalState).
    //    (The method returns a Result instead of panicking when the shared
    //    state has not been seeded.)
    shell.shared_handle::<Counter>().builtin(
        "temp2",
        builtins::builtin::<TempCommand, _>().with_state(()),
    )?;

    println!("ok");
    Ok(())
}
