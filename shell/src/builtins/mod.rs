use futures::future::BoxFuture;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::io::Write;

use crate::builtin::{
    self, BuiltinCommand, BuiltinCommandExecuteFunc, BuiltinDeclarationCommand, BuiltinResult,
};
use crate::commands::CommandArg;
use crate::context;
use crate::error;

mod alias;
mod bg;
mod brea;
mod cd;
mod colon;
mod complete;
mod continu;
mod declare;
mod dirs;
mod dot;
mod echo;
mod eval;
#[cfg(unix)]
mod exec;
mod exit;
mod export;
mod fals;
mod fg;
mod getopts;
mod help;
mod jobs;
#[cfg(unix)]
mod kill;
mod popd;
mod printf;
mod pushd;
mod pwd;
mod read;
mod retur;
mod set;
mod shift;
mod shopt;
mod test;
mod trap;
mod tru;
mod typ;
mod umask;
mod unalias;
mod unimp;
mod unset;

fn exec_builtin<T: BuiltinCommand + Send>(
    context: context::CommandExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<BuiltinResult, error::Error>> {
    Box::pin(async move { exec_builtin_impl::<T>(context, args).await })
}

async fn exec_builtin_impl<T: BuiltinCommand + Send>(
    context: context::CommandExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<BuiltinResult, error::Error> {
    let plain_args = args.into_iter().map(|arg| match arg {
        CommandArg::String(s) => s,
        CommandArg::Assignment(a) => a.to_string(),
    });

    let result = T::new(plain_args);
    let command = match result {
        Ok(command) => command,
        Err(e) => {
            writeln!(context.stderr(), "{e}")?;
            return Ok(BuiltinResult {
                exit_code: builtin::BuiltinExitCode::InvalidUsage,
            });
        }
    };

    Ok(BuiltinResult {
        exit_code: command.execute(context).await?,
    })
}

fn exec_declaration_builtin<T: BuiltinDeclarationCommand + Send>(
    context: context::CommandExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<BuiltinResult, error::Error>> {
    Box::pin(async move { exec_declaration_builtin_impl::<T>(context, args).await })
}

async fn exec_declaration_builtin_impl<T: BuiltinDeclarationCommand + Send>(
    context: context::CommandExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<BuiltinResult, error::Error> {
    let mut options = vec![];
    let mut declarations = vec![];

    for (i, arg) in args.into_iter().enumerate() {
        match arg {
            CommandArg::String(s) if i == 0 || s.starts_with('-') || s.starts_with('+') => {
                options.push(s);
            }
            _ => declarations.push(arg),
        }
    }

    let result = T::new(options);
    let mut command = match result {
        Ok(command) => command,
        Err(e) => {
            writeln!(context.stderr(), "{e}")?;
            return Ok(BuiltinResult {
                exit_code: builtin::BuiltinExitCode::InvalidUsage,
            });
        }
    };

    command.set_declarations(declarations);

    Ok(BuiltinResult {
        exit_code: command.execute(context).await?,
    })
}

lazy_static::lazy_static! {
    pub(crate) static ref SPECIAL_BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> = get_special_builtins();
    pub(crate) static ref BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> = get_builtins(true);
    pub(crate) static ref POSIX_ONLY_BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> = get_builtins(false);
    pub(crate) static ref DECLARATION_BUILTINS: HashSet<&'static str> = get_declaration_builtin_names();
}

pub(crate) fn get_all_builtin_names() -> Vec<String> {
    SPECIAL_BUILTINS
        .iter()
        .chain(BUILTINS.iter())
        .map(|(name, _)| (*name).to_owned())
        .sorted()
        .collect::<Vec<_>>()
}

fn get_declaration_builtin_names() -> HashSet<&'static str> {
    let mut s = HashSet::new();
    s.insert("alias");
    s.insert("declare");
    s.insert("export");
    s.insert("local");
    s.insert("readonly");
    s.insert("typeset");
    s
}

fn get_special_builtins() -> HashMap<&'static str, BuiltinCommandExecuteFunc> {
    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    let mut m = HashMap::<&'static str, BuiltinCommandExecuteFunc>::new();

    m.insert("break", exec_builtin::<brea::BreakCommand>);
    m.insert(":", exec_builtin::<colon::ColonCommand>);
    m.insert("continue", exec_builtin::<continu::ContinueCommand>);
    m.insert(".", exec_builtin::<dot::DotCommand>);
    m.insert("eval", exec_builtin::<eval::EvalCommand>);
    #[cfg(unix)]
    m.insert("exec", exec_builtin::<exec::ExecCommand>);
    m.insert("exit", exec_builtin::<exit::ExitCommand>);
    m.insert("export", exec_builtin::<export::ExportCommand>); // TODO: should be exec_declaration_builtin
    m.insert("readonly", exec_builtin::<unimp::UnimplementedCommand>); // TODO: should be exec_declaration_builtin
    m.insert("return", exec_builtin::<retur::ReturnCommand>);
    m.insert("set", exec_builtin::<set::SetCommand>);
    m.insert("shift", exec_builtin::<shift::ShiftCommand>);
    m.insert("times", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("trap", exec_builtin::<trap::TrapCommand>);
    m.insert("unset", exec_builtin::<unset::UnsetCommand>);

    m
}

fn get_builtins(include_extended: bool) -> HashMap<&'static str, BuiltinCommandExecuteFunc> {
    let mut m = HashMap::<&'static str, BuiltinCommandExecuteFunc>::new();

    m.insert("alias", exec_builtin::<alias::AliasCommand>); // TODO: should be exec_declaration_builtin
    m.insert("bg", exec_builtin::<bg::BgCommand>);
    m.insert("cd", exec_builtin::<cd::CdCommand>);
    m.insert("command", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("false", exec_builtin::<fals::FalseCommand>);
    m.insert("fc", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("fg", exec_builtin::<fg::FgCommand>);
    m.insert("getopts", exec_builtin::<getopts::GetOptsCommand>);
    m.insert("hash", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("help", exec_builtin::<help::HelpCommand>);
    m.insert("jobs", exec_builtin::<jobs::JobsCommand>);
    #[cfg(unix)]
    m.insert("kill", exec_builtin::<kill::KillCommand>);
    m.insert("newgrp", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("pwd", exec_builtin::<pwd::PwdCommand>);
    m.insert("read", exec_builtin::<read::ReadCommand>);
    m.insert("true", exec_builtin::<tru::TrueCommand>);
    m.insert("type", exec_builtin::<typ::TypeCommand>);
    m.insert("ulimit", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("umask", exec_builtin::<umask::UmaskCommand>);
    m.insert("unalias", exec_builtin::<unalias::UnaliasCommand>);
    m.insert("wait", exec_builtin::<unimp::UnimplementedCommand>);

    // TODO: does this belong?
    m.insert("local", exec_declaration_builtin::<declare::DeclareCommand>);

    if include_extended {
        m.insert("bind", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("builtin", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("caller", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert(
            "declare",
            exec_declaration_builtin::<declare::DeclareCommand>,
        );
        m.insert("echo", exec_builtin::<echo::EchoCommand>);
        m.insert("enable", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("let", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("logout", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("mapfile", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("printf", exec_builtin::<printf::PrintfCommand>);
        m.insert("readarray", exec_builtin::<unimp::UnimplementedCommand>);
        m.insert("shopt", exec_builtin::<shopt::ShoptCommand>);
        m.insert("source", exec_builtin::<dot::DotCommand>);
        m.insert("test", exec_builtin::<test::TestCommand>);
        m.insert("[", exec_builtin::<test::TestCommand>);
        m.insert("typeset", exec_builtin::<declare::DeclareCommand>);

        // Completion builtins
        m.insert("complete", exec_builtin::<complete::CompleteCommand>);
        m.insert("compgen", exec_builtin::<complete::CompGenCommand>);
        m.insert("compopt", exec_builtin::<complete::CompOptCommand>);

        // Dir stack builtins
        m.insert("dirs", exec_builtin::<dirs::DirsCommand>);
        m.insert("popd", exec_builtin::<popd::PopdCommand>);
        m.insert("pushd", exec_builtin::<pushd::PushdCommand>);
    }

    m
}
