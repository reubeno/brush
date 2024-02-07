use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::HashMap;

use crate::builtin::{self, BuiltinCommand, BuiltinCommandExecuteFunc, BuiltinResult};
use crate::error;

mod alias;
mod cd;
mod colon;
mod complete;
mod declare;
mod dot;
mod eval;
mod exec;
mod exit;
mod export;
mod fals;
mod help;
mod jobs;
mod pwd;
mod retur;
mod set;
mod shift;
mod shopt;
mod trap;
mod tru;
mod umask;
mod unimp;
mod unset;

fn exec_builtin<T: BuiltinCommand + Send>(
    context: builtin::BuiltinExecutionContext<'_>,
    args: Vec<String>,
) -> BoxFuture<'_, Result<BuiltinResult, error::Error>> {
    Box::pin(async move { T::execute_args(context, args).await })
}

lazy_static::lazy_static! {
    pub static ref SPECIAL_BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> = get_special_builtins();
    pub static ref BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> = get_builtins();
}

pub(crate) fn get_special_builtins() -> HashMap<&'static str, BuiltinCommandExecuteFunc> {
    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    let mut m = HashMap::<&'static str, BuiltinCommandExecuteFunc>::new();

    m.insert("break", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert(":", exec_builtin::<colon::ColonCommand>);
    m.insert("continue", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert(".", exec_builtin::<dot::DotCommand>);
    m.insert("eval", exec_builtin::<eval::EvalCommand>);
    m.insert("exec", exec_builtin::<exec::ExecCommand>);
    m.insert("exit", exec_builtin::<exit::ExitCommand>);
    m.insert("export", exec_builtin::<export::ExportCommand>);
    m.insert("readonly", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("return", exec_builtin::<retur::ReturnCommand>);
    m.insert("set", exec_builtin::<set::SetCommand>);
    m.insert("shift", exec_builtin::<shift::ShiftCommand>);
    m.insert("times", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("trap", exec_builtin::<trap::TrapCommand>);
    m.insert("unset", exec_builtin::<unset::UnsetCommand>);
    // Bash extension builtins
    m.insert("source", exec_builtin::<dot::DotCommand>);

    m
}

pub(crate) fn get_builtins() -> HashMap<&'static str, BuiltinCommandExecuteFunc> {
    let mut m = HashMap::<&'static str, BuiltinCommandExecuteFunc>::new();

    m.insert("alias", exec_builtin::<alias::AliasCommand>);
    m.insert("bg", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("cd", exec_builtin::<cd::CdCommand>);
    m.insert("command", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("false", exec_builtin::<fals::FalseCommand>);
    m.insert("fc", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("fg", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("getopts", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("hash", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("help", exec_builtin::<help::HelpCommand>);
    m.insert("jobs", exec_builtin::<jobs::JobsCommand>);
    m.insert("kill", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("newgrp", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("pwd", exec_builtin::<pwd::PwdCommand>);
    m.insert("read", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("true", exec_builtin::<tru::TrueCommand>);
    m.insert("type", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("ulimit", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("umask", exec_builtin::<umask::UmaskCommand>);
    m.insert("unalias", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("wait", exec_builtin::<unimp::UnimplementedCommand>);

    // N.B These builtins are extensions.
    // TODO: make them unavailable in sh mode.
    m.insert("declare", exec_builtin::<declare::DeclareCommand>);
    m.insert("local", exec_builtin::<declare::DeclareCommand>);
    m.insert("shopt", exec_builtin::<shopt::ShoptCommand>);
    m.insert("complete", exec_builtin::<complete::CompleteCommand>);
    m.insert("compgen", exec_builtin::<unimp::UnimplementedCommand>);
    m.insert("compopt", exec_builtin::<unimp::UnimplementedCommand>);

    m
}
