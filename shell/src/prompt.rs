use std::path::Path;

use anyhow::Result;

use crate::{error, shell::Shell};

const VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const VERSION_PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");

pub(crate) fn expand_prompt(shell: &Shell, spec: &str) -> Result<String, error::Error> {
    // Now parse.
    let prompt_pieces = parser::prompt::parse_prompt(spec)?;

    // Now render.
    let formatted_prompt = prompt_pieces
        .iter()
        .map(|p| format_prompt_piece(shell, p))
        .collect::<Result<Vec<_>, error::Error>>()?
        .join("");

    Ok(formatted_prompt)
}

pub(crate) fn format_prompt_piece(
    shell: &Shell,
    piece: &parser::prompt::PromptPiece,
) -> Result<String, error::Error> {
    let formatted = match piece {
        parser::prompt::PromptPiece::Literal(l) => l.to_owned(),
        parser::prompt::PromptPiece::AsciiCharacter(c) => {
            char::from_u32(*c).map_or_else(String::new, |c| c.to_string())
        }
        parser::prompt::PromptPiece::Backslash => "\\".to_owned(),
        parser::prompt::PromptPiece::BellCharacter => "\x07".to_owned(),
        parser::prompt::PromptPiece::CarriageReturn => "\r".to_owned(),
        parser::prompt::PromptPiece::CurrentCommandNumber => {
            return error::unimp("prompt: current command number")
        }
        parser::prompt::PromptPiece::CurrentHistoryNumber => {
            return error::unimp("prompt: current history number")
        }
        parser::prompt::PromptPiece::CurrentUser => get_current_username()?,
        parser::prompt::PromptPiece::CurrentWorkingDirectory {
            tilde_replaced,
            basename,
        } => format_current_working_directory(shell, *tilde_replaced, *basename),
        parser::prompt::PromptPiece::Date(_) => return error::unimp("prompt: date"),
        parser::prompt::PromptPiece::DollarOrPound => {
            if uzers::get_current_uid() == 0 {
                "#".to_owned()
            } else {
                "$".to_owned()
            }
        }
        parser::prompt::PromptPiece::EndNonPrintingSequence => String::new(),
        parser::prompt::PromptPiece::EscapeCharacter => "\x1b".to_owned(),
        parser::prompt::PromptPiece::Hostname {
            only_up_to_first_dot,
        } => {
            let mut hn = hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if *only_up_to_first_dot {
                if let Some((first, _)) = hn.split_once('.') {
                    hn = first.to_owned();
                }
            }
            hn
        }
        parser::prompt::PromptPiece::Newline => "\n".to_owned(),
        parser::prompt::PromptPiece::NumberOfManagedJobs => {
            return error::unimp("prompt: number of managed jobs")
        }
        parser::prompt::PromptPiece::ShellBaseName => {
            if let Some(shell_name) = &shell.shell_name {
                Path::new(shell_name)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        parser::prompt::PromptPiece::ShellRelease => {
            std::format!("{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_PATCH}")
        }
        parser::prompt::PromptPiece::ShellVersion => {
            std::format!("{VERSION_MAJOR}.{VERSION_MINOR}")
        }
        parser::prompt::PromptPiece::StartNonPrintingSequence => String::new(),
        parser::prompt::PromptPiece::TerminalDeviceBaseName => {
            return error::unimp("prompt: terminal device base name")
        }
        parser::prompt::PromptPiece::Time(_) => return error::unimp("prompt: time"),
    };

    Ok(formatted)
}

fn get_current_username() -> Result<String> {
    let username =
        uzers::get_current_username().ok_or_else(|| anyhow::anyhow!("no current user"))?;
    Ok(username.to_string_lossy().to_string())
}

fn format_current_working_directory(shell: &Shell, tilde_replaced: bool, basename: bool) -> String {
    let mut working_dir_str = shell.working_dir.to_string_lossy().to_string();

    if tilde_replaced {
        working_dir_str = shell.tilde_shorten(working_dir_str);
    }

    if basename {
        if let Some(filename) = Path::new(&working_dir_str).file_name() {
            working_dir_str = filename.to_string_lossy().to_string();
        }
    }

    working_dir_str
}
