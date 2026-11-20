use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins;

/// Identifies well-known sets of builtins.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BuiltinSet {
    /// Identifies builtins appropriate for POSIX `sh` compatibility.
    ShMode,
    /// Identifies builtins appropriate for a more full-featured `bash`-compatible shell.
    BashMode,
}

/// Returns the default set of built-in commands.
///
/// # Arguments
///
/// * `set` - The set of built-ins to return.
#[allow(clippy::too_many_lines)]
pub fn default_builtins<SE: brush_core::ShellExtensions>(
    set: BuiltinSet,
) -> HashMap<String, builtins::Registration<SE>> {
    let mut m = HashMap::<String, builtins::Registration<SE>>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    #[cfg(feature = "builtin.break")]
    m.insert(
        "break".into(),
        builtins::Registration::of::<break_::BreakCommand>().special(),
    );
    #[cfg(feature = "builtin.colon")]
    m.insert(
        ":".into(),
        builtins::Registration::of::<colon::ColonCommand>().special(),
    );
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        builtins::Registration::of::<continue_::ContinueCommand>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(
        ".".into(),
        builtins::Registration::of::<dot::DotCommand>().special(),
    );
    #[cfg(feature = "builtin.eval")]
    m.insert(
        "eval".into(),
        builtins::Registration::of::<eval::EvalCommand>().special(),
    );
    #[cfg(feature = "builtin.exec")]
    m.insert(
        "exec".into(),
        builtins::Registration::of::<exec::ExecCommand>().special(),
    );
    #[cfg(feature = "builtin.exit")]
    m.insert(
        "exit".into(),
        builtins::Registration::of::<exit::ExitCommand>().special(),
    );
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        builtins::Registration::of::<export::ExportCommand>().special(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        builtins::Registration::of::<return_::ReturnCommand>().special(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert(
        "set".into(),
        builtins::Registration::of::<set::SetCommand>().special(),
    );
    #[cfg(feature = "builtin.shift")]
    m.insert(
        "shift".into(),
        builtins::Registration::of::<shift::ShiftCommand>().special(),
    );
    #[cfg(feature = "builtin.trap")]
    m.insert(
        "trap".into(),
        builtins::Registration::of::<trap::TrapCommand>().special(),
    );
    #[cfg(feature = "builtin.unset")]
    m.insert(
        "unset".into(),
        builtins::Registration::of::<unset::UnsetCommand>().special(),
    );

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        builtins::Registration::of::<declare::DeclareCommand>().special(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert(
        "times".into(),
        builtins::Registration::of::<times::TimesCommand>().special(),
    );

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert(
        "alias".into(),
        builtins::Registration::of::<alias::AliasCommand>(),
    ); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), builtins::Registration::of::<bg::BgCommand>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), builtins::Registration::of::<cd::CdCommand>());
    #[cfg(feature = "builtin.command")]
    m.insert(
        "command".into(),
        builtins::Registration::of::<command::CommandCommand>(),
    );
    #[cfg(feature = "builtin.false")]
    m.insert(
        "false".into(),
        builtins::Registration::of::<false_::FalseCommand>(),
    );
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), builtins::Registration::of::<fg::FgCommand>());
    #[cfg(feature = "builtin.getopts")]
    m.insert(
        "getopts".into(),
        builtins::Registration::of::<getopts::GetOptsCommand>(),
    );
    #[cfg(feature = "builtin.hash")]
    m.insert(
        "hash".into(),
        builtins::Registration::of::<hash::HashCommand>(),
    );
    #[cfg(feature = "builtin.help")]
    m.insert(
        "help".into(),
        builtins::Registration::of::<help::HelpCommand>(),
    );
    #[cfg(feature = "builtin.jobs")]
    m.insert(
        "jobs".into(),
        builtins::Registration::of::<jobs::JobsCommand>(),
    );
    #[cfg(all(feature = "builtin.kill", unix))]
    m.insert(
        "kill".into(),
        builtins::Registration::of::<kill::KillCommand>(),
    );
    #[cfg(feature = "builtin.declare")]
    m.insert(
        "local".into(),
        builtins::Registration::of::<declare::DeclareCommand>(),
    );
    #[cfg(feature = "builtin.pwd")]
    m.insert(
        "pwd".into(),
        builtins::Registration::of::<pwd::PwdCommand>(),
    );
    #[cfg(feature = "builtin.read")]
    m.insert(
        "read".into(),
        builtins::Registration::of::<read::ReadCommand>(),
    );
    #[cfg(feature = "builtin.true")]
    m.insert(
        "true".into(),
        builtins::Registration::of::<true_::TrueCommand>(),
    );
    #[cfg(feature = "builtin.type")]
    m.insert(
        "type".into(),
        builtins::Registration::of::<type_::TypeCommand>(),
    );
    #[cfg(all(feature = "builtin.ulimit", unix))]
    m.insert(
        "ulimit".into(),
        builtins::Registration::of::<ulimit::ULimitCommand>(),
    );
    #[cfg(all(feature = "builtin.umask", unix))]
    m.insert(
        "umask".into(),
        builtins::Registration::of::<umask::UmaskCommand>(),
    );
    #[cfg(feature = "builtin.unalias")]
    m.insert(
        "unalias".into(),
        builtins::Registration::of::<unalias::UnaliasCommand>(),
    );
    #[cfg(feature = "builtin.wait")]
    m.insert(
        "wait".into(),
        builtins::Registration::of::<wait::WaitCommand>(),
    );

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), builtins::Registration::of::<fc::FcCommand>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert(
            "builtin".into(),
            builtins::Registration::of::<builtin_::BuiltinCommand>(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "declare".into(),
            builtins::Registration::of::<declare::DeclareCommand>(),
        );
        #[cfg(feature = "builtin.echo")]
        m.insert(
            "echo".into(),
            builtins::Registration::of::<echo::EchoCommand>(),
        );
        #[cfg(feature = "builtin.enable")]
        m.insert(
            "enable".into(),
            builtins::Registration::of::<enable::EnableCommand>(),
        );
        #[cfg(feature = "builtin.let")]
        m.insert(
            "let".into(),
            builtins::Registration::of::<let_::LetCommand>(),
        );
        #[cfg(feature = "builtin.mapfile")]
        m.insert(
            "mapfile".into(),
            builtins::Registration::of::<mapfile::MapFileCommand>(),
        );
        #[cfg(feature = "builtin.mapfile")]
        m.insert(
            "readarray".into(),
            builtins::Registration::of::<mapfile::MapFileCommand>(),
        );
        #[cfg(all(feature = "builtin.printf", any(unix, windows, target_os = "wasi")))]
        m.insert(
            "printf".into(),
            builtins::Registration::of::<printf::PrintfCommand>(),
        );
        #[cfg(feature = "builtin.shopt")]
        m.insert(
            "shopt".into(),
            builtins::Registration::of::<shopt::ShoptCommand>(),
        );
        #[cfg(feature = "builtin.dot")]
        m.insert(
            "source".into(),
            builtins::Registration::of::<dot::DotCommand>().special(),
        );
        #[cfg(all(feature = "builtin.suspend", unix))]
        m.insert(
            "suspend".into(),
            builtins::Registration::of::<suspend::SuspendCommand>(),
        );
        #[cfg(feature = "builtin.test")]
        m.insert(
            "test".into(),
            builtins::Registration::of::<test::TestCommand>(),
        );
        #[cfg(feature = "builtin.test")]
        m.insert(
            "[".into(),
            builtins::Registration::of::<test::TestCommand>(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "typeset".into(),
            builtins::Registration::of::<declare::DeclareCommand>(),
        );

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert(
            "complete".into(),
            builtins::Registration::of::<complete::CompleteCommand>(),
        );
        #[cfg(feature = "builtin.compgen")]
        m.insert(
            "compgen".into(),
            builtins::Registration::of::<complete::CompGenCommand>(),
        );
        #[cfg(feature = "builtin.compopt")]
        m.insert(
            "compopt".into(),
            builtins::Registration::of::<complete::CompOptCommand>(),
        );

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert(
            "dirs".into(),
            builtins::Registration::of::<dirs::DirsCommand>(),
        );
        #[cfg(feature = "builtin.popd")]
        m.insert(
            "popd".into(),
            builtins::Registration::of::<popd::PopdCommand>(),
        );
        #[cfg(feature = "builtin.pushd")]
        m.insert(
            "pushd".into(),
            builtins::Registration::of::<pushd::PushdCommand>(),
        );

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert(
            "bind".into(),
            builtins::Registration::of::<bind::BindCommand>(),
        );

        // History
        #[cfg(feature = "builtin.history")]
        m.insert(
            "history".into(),
            builtins::Registration::of::<history::HistoryCommand>(),
        );

        #[cfg(feature = "builtin.caller")]
        m.insert(
            "caller".into(),
            builtins::Registration::of::<caller::CallerCommand>(),
        );

        // TODO(disown): implement disown builtin
        m.insert(
            "disown".into(),
            builtins::Registration::of::<unimp::UnimplementedCommand>(),
        );

        // TODO(logout): implement logout builtin
        m.insert(
            "logout".into(),
            builtins::Registration::of::<unimp::UnimplementedCommand>(),
        );
    }

    m
}
