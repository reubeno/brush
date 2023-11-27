use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::HashMap;

use crate::builtin::{self, BuiltinCommand, BuiltinCommandExecuteFunc, BuiltinResult};

mod alias;
mod colon;
mod dot;
mod exit;
mod export;
mod pwd;
mod umask;
mod unimp;
mod unset;

fn exec_builtin<T: BuiltinCommand>(
    context: &mut builtin::BuiltinExecutionContext,
    args: &[&str],
) -> Result<BuiltinResult> {
    T::execute_args(context, args)
}

lazy_static! {
    pub static ref SPECIAL_BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> =
        HashMap::from([
            //
            // POSIX special builtins
            //
            // N.B. There seems to be some inconsistency as to whether 'times'
            // should be a special built-in.
            //
            ("break", exec_builtin::<unimp::UnimplementedCommand> as BuiltinCommandExecuteFunc),
            (":", exec_builtin::<colon::ColonCommand>),
            ("continue", exec_builtin::<unimp::UnimplementedCommand>),
            (".", exec_builtin::<dot::DotCommand>),
            ("eval", exec_builtin::<unimp::UnimplementedCommand>),
            ("exec", exec_builtin::<unimp::UnimplementedCommand>),
            ("exit", exec_builtin::<exit::ExitCommand>),
            ("export", exec_builtin::<export::ExportCommand>),
            ("readonly", exec_builtin::<unimp::UnimplementedCommand>),
            ("return", exec_builtin::<unimp::UnimplementedCommand>),
            ("set", exec_builtin::<unimp::UnimplementedCommand>),
            ("shift", exec_builtin::<unimp::UnimplementedCommand>),
            ("times", exec_builtin::<unimp::UnimplementedCommand>),
            ("trap", exec_builtin::<unimp::UnimplementedCommand>),
            ("unset", exec_builtin::<unset::UnsetCommand>),
            // Bash extension builtins
            ("source", exec_builtin::<dot::DotCommand>),
        ]);

    pub static ref BUILTINS: HashMap<&'static str, BuiltinCommandExecuteFunc> = HashMap::from([
        ("alias", exec_builtin::<alias::AliasCommand> as BuiltinCommandExecuteFunc),
        ("bg", exec_builtin::<unimp::UnimplementedCommand>),
        ("cd", exec_builtin::<unimp::UnimplementedCommand>),
        ("command", exec_builtin::<unimp::UnimplementedCommand>),
        ("false", exec_builtin::<unimp::UnimplementedCommand>),
        ("fc", exec_builtin::<unimp::UnimplementedCommand>),
        ("fg", exec_builtin::<unimp::UnimplementedCommand>),
        ("getopts", exec_builtin::<unimp::UnimplementedCommand>),
        ("hash", exec_builtin::<unimp::UnimplementedCommand>),
        ("jobs", exec_builtin::<unimp::UnimplementedCommand>),
        ("kill", exec_builtin::<unimp::UnimplementedCommand>),
        ("newgrp", exec_builtin::<unimp::UnimplementedCommand>),
        ("pwd", exec_builtin::<pwd::PwdCommand>),
        ("read", exec_builtin::<unimp::UnimplementedCommand>),
        ("true", exec_builtin::<unimp::UnimplementedCommand>),
        ("type", exec_builtin::<unimp::UnimplementedCommand>),
        ("ulimit", exec_builtin::<unimp::UnimplementedCommand>),
        ("umask", exec_builtin::<umask::UmaskCommand>),
        ("unalias", exec_builtin::<unimp::UnimplementedCommand>),
        ("wait", exec_builtin::<unimp::UnimplementedCommand>),
    ]);
}
