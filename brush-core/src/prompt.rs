use crate::{
    error,
    shell::Shell,
    sys::{self, users},
};
use std::path::Path;

const VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const VERSION_PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");

pub(crate) fn expand_prompt(shell: &Shell, spec: &str) -> Result<String, error::Error> {
    // Now parse.
    let prompt_pieces = parse_prompt(spec.to_owned())?;

    // Now render.
    let formatted_prompt = prompt_pieces
        .iter()
        .map(|p| format_prompt_piece(shell, p))
        .collect::<Result<Vec<_>, error::Error>>()?
        .join("");

    Ok(formatted_prompt)
}

#[cached::proc_macro::cached(size = 64, result = true)]
fn parse_prompt(
    spec: String,
) -> Result<Vec<brush_parser::prompt::PromptPiece>, brush_parser::WordParseError> {
    brush_parser::prompt::parse(spec.as_str())
}

pub(crate) fn format_prompt_piece(
    shell: &Shell,
    piece: &brush_parser::prompt::PromptPiece,
) -> Result<String, error::Error> {
    let formatted = match piece {
        brush_parser::prompt::PromptPiece::Literal(l) => l.to_owned(),
        brush_parser::prompt::PromptPiece::AsciiCharacter(c) => {
            char::from_u32(*c).map_or_else(String::new, |c| c.to_string())
        }
        brush_parser::prompt::PromptPiece::Backslash => "\\".to_owned(),
        brush_parser::prompt::PromptPiece::BellCharacter => "\x07".to_owned(),
        brush_parser::prompt::PromptPiece::CarriageReturn => "\r".to_owned(),
        brush_parser::prompt::PromptPiece::CurrentCommandNumber => {
            return error::unimp("prompt: current command number")
        }
        brush_parser::prompt::PromptPiece::CurrentHistoryNumber => {
            return error::unimp("prompt: current history number")
        }
        brush_parser::prompt::PromptPiece::CurrentUser => users::get_current_username()?,
        brush_parser::prompt::PromptPiece::CurrentWorkingDirectory {
            tilde_replaced,
            basename,
        } => format_current_working_directory(shell, *tilde_replaced, *basename),
        brush_parser::prompt::PromptPiece::Date(_) => return error::unimp("prompt: date"),
        brush_parser::prompt::PromptPiece::DollarOrPound => {
            if users::is_root() {
                "#".to_owned()
            } else {
                "$".to_owned()
            }
        }
        brush_parser::prompt::PromptPiece::EndNonPrintingSequence => String::new(),
        brush_parser::prompt::PromptPiece::EscapeCharacter => "\x1b".to_owned(),
        brush_parser::prompt::PromptPiece::Hostname {
            only_up_to_first_dot,
        } => {
            let hn = sys::network::get_hostname()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if *only_up_to_first_dot {
                if let Some((first, _)) = hn.split_once('.') {
                    return Ok(first.to_owned());
                }
            }
            hn
        }
        brush_parser::prompt::PromptPiece::Newline => "\n".to_owned(),
        brush_parser::prompt::PromptPiece::NumberOfManagedJobs => {
            return error::unimp("prompt: number of managed jobs")
        }
        brush_parser::prompt::PromptPiece::ShellBaseName => {
            if let Some(shell_name) = &shell.shell_name {
                Path::new(shell_name)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        brush_parser::prompt::PromptPiece::ShellRelease => {
            std::format!("{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_PATCH}")
        }
        brush_parser::prompt::PromptPiece::ShellVersion => {
            std::format!("{VERSION_MAJOR}.{VERSION_MINOR}")
        }
        brush_parser::prompt::PromptPiece::StartNonPrintingSequence => String::new(),
        brush_parser::prompt::PromptPiece::TerminalDeviceBaseName => {
            return error::unimp("prompt: terminal device base name")
        }
        brush_parser::prompt::PromptPiece::Time(_) => return error::unimp("prompt: time"),
    };

    Ok(formatted)
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

    if cfg!(windows) {
        working_dir_str = working_dir_str.replace('\\', "/");
    }

    working_dir_str
}
