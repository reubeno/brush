//! The `umask` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(UmaskCommand);

use brush_core::{ErrorKind, ExecutionResult};
use cfg_if::cfg_if;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
use nix::sys::stat::Mode;
use std::io::Write;

cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        fn get_umask() -> Result<u32, brush_core::Error> {
            let umask = procfs::process::Process::myself().ok().and_then(|me| me.status().ok()).and_then(|status| status.umask);
            umask.ok_or_else(|| brush_core::ErrorKind::InvalidUmask.into())
        }
    } else {
        #[expect(clippy::unnecessary_wraps)]
        fn get_umask() -> Result<u32, brush_core::Error> {
            let u = nix::sys::stat::umask(Mode::empty());
            nix::sys::stat::umask(u);
            Ok(u32::from(u.bits()))
        }
    }
}

pub(super) fn set_umask(value: nix::sys::stat::mode_t) -> Result<(), brush_core::Error> {
    // value of mode_t can be platform dependent
    let mode = nix::sys::stat::Mode::from_bits(value).ok_or_else(|| ErrorKind::InvalidUmask)?;
    nix::sys::stat::umask(mode);
    Ok(())
}

pub(super) fn symbolic_mask_from_bits(bits: u32) -> String {
    let mut result = String::new();

    if (bits & 0b100) != 0 {
        result.push('r');
    }
    if (bits & 0b010) != 0 {
        result.push('w');
    }
    if (bits & 0b001) != 0 {
        result.push('x');
    }

    result
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &UmaskCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if let Some(mode) = &command.mode {
        if mode.starts_with(|c: char| c.is_digit(8)) {
            let parsed = brush_core::int_utils::parse(mode.as_str(), 8)?;
            set_umask(parsed)?;
        } else {
            return brush_core::error::unimp("umask setting mode from symbolic value");
        }
    } else {
        let umask = get_umask()?;

        let formatted = if command.symbolic_output {
            let u = symbolic_mask_from_bits((!umask & 0o700) >> 6);
            let g = symbolic_mask_from_bits((!umask & 0o070) >> 3);
            let o = symbolic_mask_from_bits(!umask & 0o007);
            std::format!("u={u},g={g},o={o}")
        } else {
            std::format!("{umask:04o}")
        };

        if command.print_roundtrippable {
            writeln!(context.stdout(), "umask {formatted}")?;
        } else {
            writeln!(context.stdout(), "{formatted}")?;
        }
    }

    Ok(ExecutionResult::success())
}
