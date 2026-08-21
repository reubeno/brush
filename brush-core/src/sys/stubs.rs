#![allow(dead_code)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unnecessary_wraps)]
// These stubs stand in for real platform functionality on targets that lack it (chiefly wasm).
// They must present exactly the same signatures as the `unix`/`windows` implementations they
// substitute for -- including `async` on functions that are genuinely async elsewhere, e.g.
// `process::Child::wait`, which resolves to `tokio::process::Child::wait` on other targets. A stub
// body that never awaits is therefore the cfg-fork contract being honored, not a defect.
#![allow(clippy::unused_async)]
#![allow(
    clippy::unused_async_trait_impl,
    reason = "stubs must match the async signatures of the platform impls they stand in for"
)]
#![allow(clippy::unused_self)]

pub mod async_pipe;
pub mod commands;
pub(crate) mod env;
pub mod fd;
pub mod fs;
pub mod input;
pub(crate) mod network;
pub(crate) mod pipes;
pub mod poll;
pub mod process;
pub mod resource;
pub mod signal;
pub mod terminal;
pub(crate) mod users;
