use brush_core::builtins::{builtin, decl_builtin, raw_arg_builtin, simple_builtin};

#[allow(clippy::wildcard_imports)]
use super::*;

/// Identifies well-known sets of builtins.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BuiltinSet {
    /// Identifies builtins appropriate for POSIX `sh` compatibility.
    ShMode,
    /// Identifies builtins appropriate for a more full-featured `bash`-compatible shell.
    BashMode,
}

/// Registers the default set of built-in commands on the given shell.
///
/// # Arguments
///
/// * `shell` - The shell to register builtins on.
/// * `set` - The set of built-ins to register.
#[allow(clippy::too_many_lines)]
pub fn register_default_builtins<SE: brush_core::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
    set: BuiltinSet,
) {
    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    #[cfg(feature = "builtin.break")]
    shell.register_builtin("break", builtin::<break_::BreakCommand, SE>().special());
    #[cfg(feature = "builtin.colon")]
    shell.register_builtin(":", simple_builtin::<colon::ColonCommand, SE>().special());
    #[cfg(feature = "builtin.continue")]
    shell.register_builtin(
        "continue",
        builtin::<continue_::ContinueCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    shell.register_builtin(".", builtin::<dot::DotCommand, SE>().special());
    #[cfg(feature = "builtin.eval")]
    shell.register_builtin("eval", builtin::<eval::EvalCommand, SE>().special());
    #[cfg(all(feature = "builtin.exec", unix))]
    shell.register_builtin("exec", builtin::<exec::ExecCommand, SE>().special());
    #[cfg(feature = "builtin.exit")]
    shell.register_builtin("exit", builtin::<exit::ExitCommand, SE>().special());
    #[cfg(feature = "builtin.export")]
    shell.register_builtin(
        "export",
        decl_builtin::<export::ExportCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.return")]
    shell.register_builtin("return", builtin::<return_::ReturnCommand, SE>().special());
    #[cfg(feature = "builtin.set")]
    shell.register_builtin("set", builtin::<set::SetCommand, SE>().special());
    #[cfg(feature = "builtin.shift")]
    shell.register_builtin("shift", builtin::<shift::ShiftCommand, SE>().special());
    #[cfg(feature = "builtin.trap")]
    shell.register_builtin("trap", builtin::<trap::TrapCommand, SE>().special());
    #[cfg(feature = "builtin.unset")]
    shell.register_builtin("unset", builtin::<unset::UnsetCommand, SE>().special());

    #[cfg(feature = "builtin.declare")]
    shell.register_builtin(
        "readonly",
        decl_builtin::<declare::DeclareCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.times")]
    shell.register_builtin("times", builtin::<times::TimesCommand, SE>().special());

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    shell.register_builtin("alias", builtin::<alias::AliasCommand, SE>()); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    shell.register_builtin("bg", builtin::<bg::BgCommand, SE>());
    #[cfg(feature = "builtin.cd")]
    shell.register_builtin("cd", builtin::<cd::CdCommand, SE>());
    #[cfg(feature = "builtin.command")]
    shell.register_builtin("command", builtin::<command::CommandCommand, SE>());
    #[cfg(feature = "builtin.false")]
    shell.register_builtin("false", simple_builtin::<false_::FalseCommand, SE>());
    #[cfg(feature = "builtin.fg")]
    shell.register_builtin("fg", builtin::<fg::FgCommand, SE>());
    #[cfg(feature = "builtin.getopts")]
    shell.register_builtin("getopts", builtin::<getopts::GetOptsCommand, SE>());
    #[cfg(feature = "builtin.hash")]
    shell.register_builtin("hash", builtin::<hash::HashCommand, SE>());
    #[cfg(feature = "builtin.help")]
    shell.register_builtin("help", builtin::<help::HelpCommand, SE>());
    #[cfg(feature = "builtin.jobs")]
    shell.register_builtin("jobs", builtin::<jobs::JobsCommand, SE>());
    #[cfg(all(feature = "builtin.kill", unix))]
    shell.register_builtin("kill", builtin::<kill::KillCommand, SE>());
    #[cfg(feature = "builtin.declare")]
    shell.register_builtin("local", decl_builtin::<declare::DeclareCommand, SE>());
    #[cfg(feature = "builtin.pwd")]
    shell.register_builtin("pwd", builtin::<pwd::PwdCommand, SE>());
    #[cfg(feature = "builtin.read")]
    shell.register_builtin("read", builtin::<read::ReadCommand, SE>());
    #[cfg(feature = "builtin.true")]
    shell.register_builtin("true", simple_builtin::<true_::TrueCommand, SE>());
    #[cfg(feature = "builtin.type")]
    shell.register_builtin("type", builtin::<type_::TypeCommand, SE>());
    #[cfg(all(feature = "builtin.ulimit", unix))]
    shell.register_builtin("ulimit", builtin::<ulimit::ULimitCommand, SE>());
    #[cfg(all(feature = "builtin.umask", unix))]
    shell.register_builtin("umask", builtin::<umask::UmaskCommand, SE>());
    #[cfg(feature = "builtin.unalias")]
    shell.register_builtin("unalias", builtin::<unalias::UnaliasCommand, SE>());
    #[cfg(feature = "builtin.wait")]
    shell.register_builtin("wait", builtin::<wait::WaitCommand, SE>());

    #[cfg(feature = "builtin.fc")]
    shell.register_builtin("fc", builtin::<fc::FcCommand, SE>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        shell.register_builtin("builtin", raw_arg_builtin::<builtin_::BuiltinCommand, SE>());
        #[cfg(feature = "builtin.declare")]
        shell.register_builtin("declare", decl_builtin::<declare::DeclareCommand, SE>());
        #[cfg(feature = "builtin.echo")]
        shell.register_builtin("echo", builtin::<echo::EchoCommand, SE>());
        #[cfg(feature = "builtin.enable")]
        shell.register_builtin("enable", builtin::<enable::EnableCommand, SE>());
        #[cfg(feature = "builtin.let")]
        shell.register_builtin("let", builtin::<let_::LetCommand, SE>());
        #[cfg(feature = "builtin.mapfile")]
        shell.register_builtin("mapfile", builtin::<mapfile::MapFileCommand, SE>());
        #[cfg(feature = "builtin.mapfile")]
        shell.register_builtin("readarray", builtin::<mapfile::MapFileCommand, SE>());
        #[cfg(all(feature = "builtin.printf", any(unix, windows)))]
        shell.register_builtin("printf", builtin::<printf::PrintfCommand, SE>());
        #[cfg(feature = "builtin.shopt")]
        shell.register_builtin("shopt", builtin::<shopt::ShoptCommand, SE>());
        #[cfg(feature = "builtin.dot")]
        shell.register_builtin("source", builtin::<dot::DotCommand, SE>().special());
        #[cfg(all(feature = "builtin.suspend", unix))]
        shell.register_builtin("suspend", builtin::<suspend::SuspendCommand, SE>());
        #[cfg(feature = "builtin.test")]
        shell.register_builtin("test", builtin::<test::TestCommand, SE>());
        #[cfg(feature = "builtin.test")]
        shell.register_builtin("[", builtin::<test::TestCommand, SE>());
        #[cfg(feature = "builtin.declare")]
        shell.register_builtin("typeset", decl_builtin::<declare::DeclareCommand, SE>());

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        shell.register_builtin("complete", builtin::<complete::CompleteCommand, SE>());
        #[cfg(feature = "builtin.compgen")]
        shell.register_builtin("compgen", builtin::<complete::CompGenCommand, SE>());
        #[cfg(feature = "builtin.compopt")]
        shell.register_builtin("compopt", builtin::<complete::CompOptCommand, SE>());

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        shell.register_builtin("dirs", builtin::<dirs::DirsCommand, SE>());
        #[cfg(feature = "builtin.popd")]
        shell.register_builtin("popd", builtin::<popd::PopdCommand, SE>());
        #[cfg(feature = "builtin.pushd")]
        shell.register_builtin("pushd", builtin::<pushd::PushdCommand, SE>());

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        shell.register_builtin("bind", builtin::<bind::BindCommand, SE>());

        // History
        #[cfg(feature = "builtin.history")]
        shell.register_builtin("history", builtin::<history::HistoryCommand, SE>());

        #[cfg(feature = "builtin.caller")]
        shell.register_builtin("caller", builtin::<caller::CallerCommand, SE>());

        // TODO(disown): implement disown builtin
        shell.register_builtin("disown", builtin::<unimp::UnimplementedCommand, SE>());

        // TODO(logout): implement logout builtin
        shell.register_builtin("logout", builtin::<unimp::UnimplementedCommand, SE>());
    }
}
