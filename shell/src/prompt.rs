use anyhow::Result;

use crate::context::ExecutionContext;

pub(crate) fn format_prompt_piece(
    context: &ExecutionContext,
    piece: &parser::prompt::ShellPromptPiece,
) -> Result<String> {
    let formatted = match piece {
        parser::prompt::ShellPromptPiece::Literal(l) => l.to_owned(),
        parser::prompt::ShellPromptPiece::AsciiCharacter(_) => todo!("prompt: ascii char"),
        parser::prompt::ShellPromptPiece::Backslash => "\\".to_owned(),
        parser::prompt::ShellPromptPiece::BellCharacter => todo!("bell character"),
        parser::prompt::ShellPromptPiece::CarriageReturn => "\r".to_owned(),
        parser::prompt::ShellPromptPiece::CurrentCommandNumber => {
            todo!("prompt: current command number")
        }
        parser::prompt::ShellPromptPiece::CurrentHistoryNumber => {
            todo!("prompt: current history number")
        }
        parser::prompt::ShellPromptPiece::CurrentUser => get_current_username()?,
        parser::prompt::ShellPromptPiece::CurrentWorkingDirectory {
            tilde_replaced,
            basename,
        } => format_current_working_directory(context, *tilde_replaced, *basename)?,
        parser::prompt::ShellPromptPiece::Date(_) => todo!("prompt: date"),
        parser::prompt::ShellPromptPiece::DollarOrPound => {
            if users::get_current_uid() == 0 {
                "#".to_owned()
            } else {
                "$".to_owned()
            }
        }
        parser::prompt::ShellPromptPiece::EndNonPrintingSequence => "".to_owned(),
        parser::prompt::ShellPromptPiece::EscapeCharacter => "\x1b".to_owned(),
        parser::prompt::ShellPromptPiece::Hostname {
            only_up_to_first_dot: _,
        } => todo!("prompt: hostname"),
        parser::prompt::ShellPromptPiece::Newline => "\n".to_owned(),
        parser::prompt::ShellPromptPiece::NumberOfManagedJobs => {
            todo!("prompt: number of managed jobs")
        }
        parser::prompt::ShellPromptPiece::ShellBaseName => todo!("prompt: shell base name"),
        parser::prompt::ShellPromptPiece::ShellRelease => todo!("prompt: shell release"),
        parser::prompt::ShellPromptPiece::ShellVersion => todo!("prompt: shell version"),
        parser::prompt::ShellPromptPiece::StartNonPrintingSequence => "".to_owned(),
        parser::prompt::ShellPromptPiece::TerminalDeviceBaseName => {
            todo!("prompt: terminal device base name")
        }
        parser::prompt::ShellPromptPiece::Time(_) => todo!("prompt: time"),
    };

    Ok(formatted)
}

fn get_current_username() -> Result<String> {
    let username =
        users::get_current_username().ok_or_else(|| anyhow::anyhow!("no current user"))?;
    Ok(username.to_string_lossy().to_string())
}

fn format_current_working_directory(
    context: &ExecutionContext,
    tilde_replaced: bool,
    basename: bool,
) -> Result<String> {
    let mut working_dir_str = context.working_dir.to_string_lossy().to_string();

    if basename {
        todo!("prompt: basename of working dir");
    }

    if tilde_replaced {
        let home_dir_opt = context.parameters.get("HOME");
        if let Some(home_dir) = home_dir_opt {
            if let Some(stripped) = working_dir_str.strip_prefix(home_dir) {
                working_dir_str = format!("~{}", stripped);
            }
        }
    }

    Ok(working_dir_str)
}