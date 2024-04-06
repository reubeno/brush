use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct EchoCommand {
    #[arg(short = 'n')]
    no_trailing_newline: bool,

    #[arg(short = 'e')]
    interpret_backslash_escapes: bool,

    #[arg(short = 'E')]
    no_interpret_backslash_escapes: bool,

    /// Command and args.
    #[clap(allow_hyphen_values = true)]
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for EchoCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut trailing_newline = !self.no_trailing_newline;
        let mut s;
        if self.interpret_backslash_escapes {
            s = String::new();
            for (i, arg) in self.args.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }

                let (expanded_arg, keep_going) = expand_backslash_escapes(arg.as_str())?;
                s.push_str(expanded_arg.as_str());

                if !keep_going {
                    trailing_newline = false;
                    break;
                }
            }
        } else {
            s = self.args.join(" ");
        }

        if trailing_newline {
            s.push('\n');
        }

        write!(context.stdout(), "{s}")?;
        return Ok(BuiltinExitCode::Success);
    }
}

#[allow(clippy::too_many_lines)]
fn expand_backslash_escapes(s: &str) -> Result<(String, bool), crate::error::Error> {
    let mut result = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(next) = it.next() {
                match next {
                    'a' => result.push('\x07'),
                    'b' => result.push('\x08'),
                    'c' => {
                        // Stop all additional output!
                        return Ok((result, false));
                    }
                    'e' | 'E' => result.push('\x1b'),
                    'f' => result.push('\x0c'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    'v' => result.push('\x0b'),
                    '\\' => result.push('\\'),
                    '0' => {
                        // Consume 0-3 valid octal chars
                        let mut taken_so_far = 0;
                        let mut octal_chars: String = it
                            .take_while_ref(|c| {
                                if taken_so_far < 3 && matches!(*c, '0'..='7') {
                                    taken_so_far += 1;
                                    true
                                } else {
                                    false
                                }
                            })
                            .collect();

                        if octal_chars.is_empty() {
                            octal_chars.push('0');
                        }

                        let value = u8::from_str_radix(octal_chars.as_str(), 8)?;

                        // TODO: Should really parse as ASCII.
                        result.push_str(
                            std::str::from_utf8(&[value])
                                .map_err(|e| crate::error::Error::Unknown(e.into()))?,
                        );
                    }
                    'x' => {
                        // Consume 1-2 valid hex chars
                        let mut taken_so_far = 0;
                        let hex_chars: String = it
                            .take_while_ref(|c| {
                                if taken_so_far < 2 && c.is_ascii_hexdigit() {
                                    taken_so_far += 1;
                                    true
                                } else {
                                    false
                                }
                            })
                            .collect();

                        if hex_chars.is_empty() {
                            result.push('\\');
                            result.push(c);
                        } else {
                            let value = u8::from_str_radix(hex_chars.as_str(), 16)?;

                            // TODO: Should really parse as ASCII.
                            result.push_str(
                                std::str::from_utf8(&[value])
                                    .map_err(|e| crate::error::Error::Unknown(e.into()))?,
                            );
                        }
                    }
                    'u' => {
                        // Consume 1-4 hex digits
                        let mut taken_so_far = 0;
                        let hex_chars: String = it
                            .take_while_ref(|c| {
                                if taken_so_far < 4 && c.is_ascii_hexdigit() {
                                    taken_so_far += 1;
                                    true
                                } else {
                                    false
                                }
                            })
                            .collect();

                        if hex_chars.is_empty() {
                            result.push('\\');
                            result.push(c);
                        } else {
                            let value = u16::from_str_radix(hex_chars.as_str(), 16)?;

                            if let Some(decoded) = char::from_u32(u32::from(value)) {
                                result.push(decoded);
                            } else {
                                result.push('\\');
                                result.push(c);
                            }
                        }
                    }
                    'U' => {
                        // Consume 1-8 hex digits
                        let mut taken_so_far = 0;
                        let hex_chars: String = it
                            .take_while_ref(|c| {
                                if taken_so_far < 8 && c.is_ascii_hexdigit() {
                                    taken_so_far += 1;
                                    true
                                } else {
                                    false
                                }
                            })
                            .collect();

                        if hex_chars.is_empty() {
                            result.push('\\');
                            result.push(c);
                        } else {
                            let value = u32::from_str_radix(hex_chars.as_str(), 16)?;

                            if let Some(decoded) = char::from_u32(value) {
                                result.push(decoded);
                            } else {
                                result.push('\\');
                                result.push(c);
                            }
                        }
                    }
                    _ => result.push(c),
                }
            } else {
                // It's a trailing backslash, add it.
                result.push(c);
            }
        } else {
            // Not a backslash, add and move on.
            result.push(c);
        }
    }

    Ok((result, true))
}
