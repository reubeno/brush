use lazy_static::lazy_static;
use std::collections::HashMap;

use crate::context::BuiltinCommand;

mod alias;
mod colon;
mod dot;
mod pwd;
mod unimp;

lazy_static! {
    pub static ref SPECIAL_BUILTINS: HashMap<&'static str, BuiltinCommand> =
        HashMap::from([
            //
            // POSIX special builtins
            //
            // N.B. There seems to be some inconsistency as to whether 'times'
            // should be a special built-in.
            //
            ("break", unimp::builtin_unimplemented as BuiltinCommand),
            (":", colon::builtin_colon),
            ("continue", unimp::builtin_unimplemented),
            (".", dot::builtin_dot),
            ("eval", unimp::builtin_unimplemented),
            ("exec", unimp::builtin_unimplemented),
            ("exit", unimp::builtin_unimplemented),
            ("export", unimp::builtin_unimplemented),
            ("readonly", unimp::builtin_unimplemented),
            ("return", unimp::builtin_unimplemented),
            ("set", unimp::builtin_unimplemented),
            ("shift", unimp::builtin_unimplemented),
            ("times", unimp::builtin_unimplemented),
            ("trap", unimp::builtin_unimplemented),
            ("unset", unimp::builtin_unimplemented),
            // Bash extension builtins
            ("source", dot::builtin_dot),
        ]);

    pub static ref BUILTINS: HashMap<&'static str, BuiltinCommand> = HashMap::from([
        ("alias", alias::builtin_alias as BuiltinCommand),
        ("bg", unimp::builtin_unimplemented),
        ("cd", unimp::builtin_unimplemented),
        ("command", unimp::builtin_unimplemented),
        ("false", unimp::builtin_unimplemented),
        ("fc", unimp::builtin_unimplemented),
        ("fg", unimp::builtin_unimplemented),
        ("getopts", unimp::builtin_unimplemented),
        ("hash", unimp::builtin_unimplemented),
        ("jobs", unimp::builtin_unimplemented),
        ("kill", unimp::builtin_unimplemented),
        ("newgrp", unimp::builtin_unimplemented),
        ("pwd", pwd::builtin_pwd),
        ("read", unimp::builtin_unimplemented),
        ("true", unimp::builtin_unimplemented),
        ("type", unimp::builtin_unimplemented),
        ("ulimit", unimp::builtin_unimplemented),
        ("umask", unimp::builtin_unimplemented),
        ("unalias", unimp::builtin_unimplemented),
        ("wait", unimp::builtin_unimplemented),
    ]);
}
