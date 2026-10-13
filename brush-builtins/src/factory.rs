use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins::{self, Registration};

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
        Registration::stateless::<break_::BreakCommand>(),
    );
    #[cfg(feature = "builtin.colon")]
    m.insert(":".into(), Registration::stateless::<colon::ColonCommand>());
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        Registration::stateless::<continue_::ContinueCommand>(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(".".into(), Registration::stateless::<dot::DotCommand>());
    #[cfg(feature = "builtin.eval")]
    m.insert(
        "eval".into(),
        Registration::stateless::<eval::EvalCommand>(),
    );
    #[cfg(feature = "builtin.exec")]
    m.insert(
        "exec".into(),
        Registration::stateless::<exec::ExecCommand>(),
    );
    #[cfg(feature = "builtin.exit")]
    m.insert(
        "exit".into(),
        Registration::stateless::<exit::ExitCommand>(),
    );
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        Registration::stateless::<export::ExportCommand>(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        Registration::stateless::<return_::ReturnCommand>(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert("set".into(), Registration::stateless::<set::SetCommand>());
    #[cfg(feature = "builtin.shift")]
    m.insert(
        "shift".into(),
        Registration::stateless::<shift::ShiftCommand>(),
    );
    #[cfg(feature = "builtin.trap")]
    m.insert(
        "trap".into(),
        Registration::stateless::<trap::TrapCommand>(),
    );
    #[cfg(feature = "builtin.unset")]
    m.insert(
        "unset".into(),
        Registration::stateless::<unset::UnsetCommand>(),
    );

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        Registration::stateless::<declare::DeclareCommand>(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert(
        "times".into(),
        Registration::stateless::<times::TimesCommand>(),
    );

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert(
        "alias".into(),
        Registration::stateless::<alias::AliasCommand>(),
    ); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), Registration::stateless::<bg::BgCommand>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), Registration::stateless::<cd::CdCommand>());
    #[cfg(feature = "builtin.command")]
    m.insert(
        "command".into(),
        Registration::stateless::<command::CommandCommand>(),
    );
    #[cfg(feature = "builtin.false")]
    m.insert(
        "false".into(),
        Registration::stateless::<false_::FalseCommand>(),
    );
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), Registration::stateless::<fg::FgCommand>());
    #[cfg(feature = "builtin.getopts")]
    m.insert(
        "getopts".into(),
        Registration::stateless::<getopts::GetOptsCommand>(),
    );
    #[cfg(feature = "builtin.hash")]
    m.insert(
        "hash".into(),
        Registration::stateless::<hash::HashCommand>(),
    );
    #[cfg(feature = "builtin.help")]
    m.insert(
        "help".into(),
        Registration::stateless::<help::HelpCommand>(),
    );
    #[cfg(feature = "builtin.jobs")]
    m.insert(
        "jobs".into(),
        Registration::stateless::<jobs::JobsCommand>(),
    );
    #[cfg(all(feature = "builtin.kill", unix))]
    m.insert(
        "kill".into(),
        Registration::stateless::<kill::KillCommand>(),
    );
    #[cfg(feature = "builtin.declare")]
    m.insert(
        "local".into(),
        Registration::stateless::<declare::DeclareCommand>(),
    );
    #[cfg(feature = "builtin.pwd")]
    m.insert("pwd".into(), Registration::stateless::<pwd::PwdCommand>());
    #[cfg(feature = "builtin.read")]
    m.insert(
        "read".into(),
        Registration::stateless::<read::ReadCommand>(),
    );
    #[cfg(feature = "builtin.true")]
    m.insert(
        "true".into(),
        Registration::stateless::<true_::TrueCommand>(),
    );
    #[cfg(feature = "builtin.type")]
    m.insert(
        "type".into(),
        Registration::stateless::<type_::TypeCommand>(),
    );
    #[cfg(all(feature = "builtin.ulimit", unix))]
    m.insert(
        "ulimit".into(),
        Registration::stateless::<ulimit::ULimitCommand>(),
    );
    #[cfg(all(feature = "builtin.umask", unix))]
    m.insert(
        "umask".into(),
        Registration::stateless::<umask::UmaskCommand>(),
    );
    #[cfg(feature = "builtin.unalias")]
    m.insert(
        "unalias".into(),
        Registration::stateless::<unalias::UnaliasCommand>(),
    );
    #[cfg(feature = "builtin.wait")]
    m.insert(
        "wait".into(),
        Registration::stateless::<wait::WaitCommand>(),
    );

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), Registration::stateless::<fc::FcCommand>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert(
            "builtin".into(),
            Registration::stateless::<builtin_::BuiltinCommand>(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "declare".into(),
            Registration::stateless::<declare::DeclareCommand>(),
        );
        #[cfg(feature = "builtin.echo")]
        m.insert(
            "echo".into(),
            Registration::stateless::<echo::EchoCommand>(),
        );
        #[cfg(feature = "builtin.enable")]
        m.insert(
            "enable".into(),
            Registration::stateless::<enable::EnableCommand>(),
        );
        #[cfg(feature = "builtin.let")]
        m.insert("let".into(), Registration::stateless::<let_::LetCommand>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert(
            "mapfile".into(),
            Registration::stateless::<mapfile::MapFileCommand>(),
        );
        #[cfg(feature = "builtin.mapfile")]
        m.insert(
            "readarray".into(),
            Registration::stateless::<mapfile::MapFileCommand>(),
        );
        #[cfg(all(feature = "builtin.printf", any(unix, windows)))]
        m.insert(
            "printf".into(),
            Registration::stateless::<printf::PrintfCommand>(),
        );
        #[cfg(feature = "builtin.shopt")]
        m.insert(
            "shopt".into(),
            Registration::stateless::<shopt::ShoptCommand>(),
        );
        #[cfg(feature = "builtin.dot")]
        m.insert(
            "source".into(),
            Registration::stateless::<dot::DotCommand>(),
        );
        #[cfg(all(feature = "builtin.suspend", unix))]
        m.insert(
            "suspend".into(),
            Registration::stateless::<suspend::SuspendCommand>(),
        );
        #[cfg(feature = "builtin.test")]
        m.insert(
            "test".into(),
            Registration::stateless::<test::TestCommand>(),
        );
        #[cfg(feature = "builtin.test")]
        m.insert("[".into(), Registration::stateless::<test::TestCommand>());
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "typeset".into(),
            Registration::stateless::<declare::DeclareCommand>(),
        );

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert(
            "complete".into(),
            Registration::stateless::<complete::CompleteCommand>(),
        );
        #[cfg(feature = "builtin.compgen")]
        m.insert(
            "compgen".into(),
            Registration::stateless::<complete::CompGenCommand>(),
        );
        #[cfg(feature = "builtin.compopt")]
        m.insert(
            "compopt".into(),
            Registration::stateless::<complete::CompOptCommand>(),
        );

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert(
            "dirs".into(),
            Registration::stateless::<dirs::DirsCommand>(),
        );
        #[cfg(feature = "builtin.popd")]
        m.insert(
            "popd".into(),
            Registration::stateless::<popd::PopdCommand>(),
        );
        #[cfg(feature = "builtin.pushd")]
        m.insert(
            "pushd".into(),
            Registration::stateless::<pushd::PushdCommand>(),
        );

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert(
            "bind".into(),
            Registration::stateless::<bind::BindCommand>(),
        );

        // History
        #[cfg(feature = "builtin.history")]
        m.insert(
            "history".into(),
            Registration::stateless::<history::HistoryCommand>(),
        );

        #[cfg(feature = "builtin.caller")]
        m.insert(
            "caller".into(),
            Registration::stateless::<caller::CallerCommand>(),
        );

        // TODO(disown): implement disown builtin
        m.insert(
            "disown".into(),
            Registration::stateless::<unimp::UnimplementedCommand>(),
        );

        // TODO(logout): implement logout builtin
        m.insert(
            "logout".into(),
            Registration::stateless::<unimp::UnimplementedCommand>(),
        );
    }

    m
}
