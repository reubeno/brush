use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct CompleteCommand {
    #[arg(short = 'p')]
    print: bool,

    #[arg(short = 'r')]
    remove: bool,

    #[arg(short = 'D')]
    use_as_default: bool,

    #[arg(short = 'E')]
    use_for_empty_line: bool,

    #[arg(short = 'I')]
    use_for_initial_word: bool,

    #[arg(short = 'o')]
    option: Option<String>,

    #[arg(short = 'A')]
    action: Option<String>,

    #[arg(short = 'G')]
    globpat: Option<String>,

    #[arg(short = 'W')]
    wordlist: Option<String>,

    #[arg(short = 'F')]
    function: Option<String>,

    #[arg(short = 'C')]
    command: Option<String>,

    #[arg(short = 'X')]
    filterpat: Option<String>,

    #[arg(short = 'P')]
    prefix: Option<String>,

    #[arg(short = 'S')]
    suffix: Option<String>,

    #[arg(short = 'a')]
    action_alias: bool,

    #[arg(short = 'b')]
    action_builtin: bool,

    #[arg(short = 'c')]
    action_command: bool,

    #[arg(short = 'd')]
    action_directory: bool,

    #[arg(short = 'e')]
    action_exported: bool,

    #[arg(short = 'f')]
    action_file: bool,

    #[arg(short = 'g')]
    action_group: bool,

    #[arg(short = 'j')]
    action_job: bool,

    #[arg(short = 'k')]
    action_keyword: bool,

    #[arg(short = 's')]
    action_service: bool,

    #[arg(short = 'u')]
    action_user: bool,

    #[arg(short = 'v')]
    action_variable: bool,

    names: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for CompleteCommand {
    async fn execute(
        &self,
        _context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.print {
            log::debug!("UNIMPLEMENTED: complete -p");
        }

        if self.remove {
            log::debug!("UNIMPLEMENTED: complete -r");
        }

        if self.use_as_default {
            log::debug!("UNIMPLEMENTED: complete -D");
        }

        if self.use_for_empty_line {
            log::debug!("UNIMPLEMENTED: complete -E");
        }

        if self.use_for_initial_word {
            log::debug!("UNIMPLEMENTED: complete -I");
        }

        if let Some(option) = &self.option {
            log::debug!("UNIMPLEMENTED: complete -o {}", option);
        }

        if let Some(action) = &self.action {
            log::debug!("UNIMPLEMENTED: complete -A {}", action);
        }

        if let Some(globpat) = &self.globpat {
            log::debug!("UNIMPLEMENTED: complete -G {}", globpat);
        }

        if let Some(wordlist) = &self.wordlist {
            log::debug!("UNIMPLEMENTED: complete -W {}", wordlist);
        }

        if let Some(function) = &self.function {
            log::debug!("UNIMPLEMENTED: complete -F {}", function);
        }

        if let Some(command) = &self.command {
            log::debug!("UNIMPLEMENTED: complete -C {}", command);
        }

        if let Some(filterpat) = &self.filterpat {
            log::debug!("UNIMPLEMENTED: complete -X {}", filterpat);
        }

        if let Some(prefix) = &self.prefix {
            log::debug!("UNIMPLEMENTED: complete -P {}", prefix);
        }

        if let Some(suffix) = &self.suffix {
            log::debug!("UNIMPLEMENTED: complete -S {}", suffix);
        }

        if self.action_alias {
            log::debug!("UNIMPLEMENTED: complete -a");
        }

        if self.action_builtin {
            log::debug!("UNIMPLEMENTED: complete -b");
        }

        if self.action_command {
            log::debug!("UNIMPLEMENTED: complete -c");
        }

        if self.action_directory {
            log::debug!("UNIMPLEMENTED: complete -d");
        }

        if self.action_exported {
            log::debug!("UNIMPLEMENTED: complete -e");
        }

        if self.action_file {
            log::debug!("UNIMPLEMENTED: complete -f");
        }

        if self.action_group {
            log::debug!("UNIMPLEMENTED: complete -g");
        }

        if self.action_job {
            log::debug!("UNIMPLEMENTED: complete -j");
        }

        if self.action_keyword {
            log::debug!("UNIMPLEMENTED: complete -k");
        }

        if self.action_service {
            log::debug!("UNIMPLEMENTED: complete -s");
        }

        if self.action_user {
            log::debug!("UNIMPLEMENTED: complete -u");
        }

        if self.action_variable {
            log::debug!("UNIMPLEMENTED: complete -v");
        }

        if !self.names.is_empty() {
            // TODO: implement complete command
        } else {
            log::debug!("UNIMPLEMENTED: complete (no names)");
        }

        Ok(BuiltinExitCode::Success)
    }
}
