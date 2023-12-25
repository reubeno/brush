use anyhow::Result;

use crate::shell::Shell;
pub(crate) fn format_prompt_piece(
    shell: &Shell,
    piece: &parser::prompt::PromptPiece,
) -> Result<String> {
    let formatted = match piece {
        parser::prompt::PromptPiece::Literal(l) => l.to_owned(),
        parser::prompt::PromptPiece::AsciiCharacter(c) => {
            char::from_u32(*c).map_or_else(String::new, |c| c.to_string())
        }
        parser::prompt::PromptPiece::Backslash => "\\".to_owned(),
        parser::prompt::PromptPiece::BellCharacter => "\x07".to_owned(),
        parser::prompt::PromptPiece::CarriageReturn => "\r".to_owned(),
        parser::prompt::PromptPiece::CurrentCommandNumber => {
            todo!("prompt: current command number")
        }
        parser::prompt::PromptPiece::CurrentHistoryNumber => {
            todo!("prompt: current history number")
        }
        parser::prompt::PromptPiece::CurrentUser => get_current_username()?,
        parser::prompt::PromptPiece::CurrentWorkingDirectory {
            tilde_replaced,
            basename,
        } => format_current_working_directory(shell, *tilde_replaced, *basename)?,
        parser::prompt::PromptPiece::Date(_) => todo!("prompt: date"),
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
            todo!("prompt: number of managed jobs")
        }
        parser::prompt::PromptPiece::ShellBaseName => todo!("prompt: shell base name"),
        parser::prompt::PromptPiece::ShellRelease => todo!("prompt: shell release"),
        parser::prompt::PromptPiece::ShellVersion => todo!("prompt: shell version"),
        parser::prompt::PromptPiece::StartNonPrintingSequence => String::new(),
        parser::prompt::PromptPiece::TerminalDeviceBaseName => {
            todo!("prompt: terminal device base name")
        }
        parser::prompt::PromptPiece::Time(_) => todo!("prompt: time"),
    };

    Ok(formatted)
}

fn get_current_username() -> Result<String> {
    let username =
        uzers::get_current_username().ok_or_else(|| anyhow::anyhow!("no current user"))?;
    Ok(username.to_string_lossy().to_string())
}

fn format_current_working_directory(
    shell: &Shell,
    tilde_replaced: bool,
    basename: bool,
) -> Result<String> {
    let mut working_dir_str = shell.working_dir.to_string_lossy().to_string();

    if basename {
        todo!("prompt: basename of working dir");
    }

    if tilde_replaced {
        let home_dir_opt = shell.env.get("HOME");
        if let Some(home_dir) = home_dir_opt {
            if let Some(stripped) = working_dir_str.strip_prefix(&String::from(&home_dir.value)) {
                working_dir_str = format!("~{}", stripped);
            }
        }
    }

    Ok(working_dir_str)
}
