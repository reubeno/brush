use brush_core::{ErrorKind, ExecutionResult, builtins};
use cfg_if::cfg_if;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
use nix::sys::stat::Mode;
use std::io::Write;

/// Manage the process umask.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct UmaskCommand {
    /// If MODE is omitted, output in a form that may be reused as input.
    #[arg(short = 'p')]
    print_roundtrippable: bool,

    /// Makes the output symbolic; otherwise an octal number is given.
    #[arg(short = 'S')]
    symbolic_output: bool,

    /// Mode mask.
    #[arg(positional)]
    mode: Option<String>,
}

brush_winnow_args_adapter::impl_parse_args!(UmaskCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for UmaskCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(mode) = &self.mode {
            if mode.starts_with(|c: char| c.is_ascii_digit()) {
                // Like bash, take any mode that starts with a digit as octal, and refuse
                // one with a digit that isn't octal or a value past 07777.
                let parsed = brush_core::int_utils::parse::<nix::sys::stat::mode_t>(mode, 8)
                    .ok()
                    .filter(|value| *value <= 0o7777);

                let Some(parsed) = parsed else {
                    writeln!(
                        context.stderr(),
                        "{}: {mode}: octal number out of range",
                        context.command_name
                    )?;
                    return Ok(ExecutionResult::general_error());
                };

                set_umask(parsed)?;
            } else {
                let current = get_umask()?;
                match apply_symbolic_mode(mode, current) {
                    Ok(new_umask) => {
                        // `mode_t` is `u32` on some platforms and narrower on others.
                        #[allow(clippy::useless_conversion)]
                        let new_umask = new_umask.try_into().unwrap_or(0o777);
                        set_umask(new_umask)?;
                    }
                    Err(message) => {
                        writeln!(context.stderr(), "{}: {message}", context.command_name)?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
            }
        } else {
            let umask = get_umask()?;

            let formatted = if self.symbolic_output {
                let u = symbolic_mask_from_bits((!umask & 0o700) >> 6);
                let g = symbolic_mask_from_bits((!umask & 0o070) >> 3);
                let o = symbolic_mask_from_bits(!umask & 0o007);
                std::format!("u={u},g={g},o={o}")
            } else {
                std::format!("{umask:04o}")
            };

            if self.print_roundtrippable {
                writeln!(context.stdout(), "umask {formatted}")?;
            } else {
                writeln!(context.stdout(), "{formatted}")?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

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

fn set_umask(value: nix::sys::stat::mode_t) -> Result<(), brush_core::Error> {
    // value of mode_t can be platform dependent
    let mode = nix::sys::stat::Mode::from_bits(value).ok_or_else(|| ErrorKind::InvalidUmask)?;
    nix::sys::stat::umask(mode);
    Ok(())
}

/// Applies a symbolic mode (e.g., `g-w,o=rx`) to the given umask, returning the new umask.
/// Copy sources (`u`, `g`, `o`) refer to the original permissions, not those updated by earlier
/// clauses. Symbolic modes describe the *permitted* bits, which are the complement of the umask.
fn apply_symbolic_mode(mode: &str, umask: u32) -> Result<u32, String> {
    let original = !umask & 0o777;
    let mut perms = original;

    for clause in mode.split(',') {
        let mut chars = clause.chars().peekable();

        let mut who = 0u32;
        while let Some(shift) = chars.peek().and_then(|c| match c {
            'u' => Some(0o700),
            'g' => Some(0o070),
            'o' => Some(0o007),
            'a' => Some(0o777),
            _ => None,
        }) {
            who |= shift;
            chars.next();
        }
        if who == 0 {
            who = 0o777;
        }

        // At least one operator is required; more may follow.
        if !matches!(chars.peek(), Some('+' | '-' | '=')) {
            let found = chars.peek().copied().unwrap_or(' ');
            return Err(std::format!("`{found}': invalid symbolic mode operator"));
        }

        while let Some(op @ ('+' | '-' | '=')) = chars.peek().copied() {
            chars.next();

            let mut bits = 0u32;
            while let Some(c) = chars.peek().copied() {
                match c {
                    'r' => bits |= 0o444,
                    'w' => bits |= 0o222,
                    'x' => bits |= 0o111,
                    // Conditional execute: only applies if some class had execute originally.
                    'X' if original & 0o111 != 0 => bits |= 0o111,
                    // Accepted, but have no effect on a umask.
                    's' | 't' | 'X' => (),
                    'u' | 'g' | 'o' => {
                        let source = match c {
                            'u' => (original >> 6) & 0o7,
                            'g' => (original >> 3) & 0o7,
                            _ => original & 0o7,
                        };
                        bits = source * 0o111;
                    }
                    '+' | '-' | '=' => break,
                    _ => {
                        return Err(std::format!("`{c}': invalid symbolic mode character"));
                    }
                }
                chars.next();
            }

            let bits = bits & who;
            match op {
                '+' => perms |= bits,
                '-' => perms &= !bits,
                _ => perms = (perms & !who) | bits,
            }
        }
    }

    Ok(!perms & 0o777)
}

fn symbolic_mask_from_bits(bits: u32) -> String {
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
