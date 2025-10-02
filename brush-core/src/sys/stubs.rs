#![allow(dead_code)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::unused_async)]
#![allow(clippy::unused_self)]

pub mod fs;
pub mod input;
pub(crate) mod network;
pub(crate) mod pipes;
pub(crate) mod process;
pub mod resource;
pub mod signal;
pub mod terminal;
pub(crate) mod users;
