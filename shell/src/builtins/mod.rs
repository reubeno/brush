use futures::future::BoxFuture;
use std::collections::HashMap;
use std::io::Write;

use crate::builtin::{
    self, BuiltinCommand, BuiltinDeclarationCommand, BuiltinRegistration, BuiltinResult,
};
use crate::commands::CommandArg;
use crate::context;
use crate::error;

mod alias;
mod bg;
mod break_;
mod builtin_;
mod cd;
mod colon;
mod complete;
mod continue_;
mod declare;
mod dirs;
mod dot;
mod echo;
mod enable;
mod eval;
#[cfg(unix)]
mod exec;
mod exit;
mod export;
mod false_;
mod fg;
mod getopts;
mod help;
mod jobs;
#[cfg(unix)]
mod kill;
mod let_;
mod popd;
mod printf;
mod pushd;
mod pwd;
mod read;
mod return_;
mod set;
mod shift;
mod shopt;
mod test;
mod trap;
mod true_;
mod type_;
mod umask;
mod unalias;
mod unimp;
mod unset;
mod wait;

fn builtin<B: BuiltinCommand + Send>() -> BuiltinRegistration {
    BuiltinRegistration {
        execute_func: exec_builtin::<B>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
    }
}

fn special_builtin<B: BuiltinCommand + Send>() -> BuiltinRegistration {
    BuiltinRegistration {
        execute_func: exec_builtin::<B>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: true,
        declaration_builtin: false,
    }
}

fn decl_builtin<B: BuiltinDeclarationCommand + Send>() -> BuiltinRegistration {
    BuiltinRegistration {
        execute_func: exec_declaration_builtin::<B>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: true,
    }
}

fn special_decl_builtin<B: BuiltinDeclarationCommand + Send>() -> BuiltinRegistration {
    BuiltinRegistration {
        execute_func: exec_declaration_builtin::<B>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: true,
        declaration_builtin: true,
    }
}

fn get_builtin_content<T: BuiltinCommand + Send>(
    name: &str,
    content_type: builtin::BuiltinContentType,
) -> String {
    T::get_content(name, content_type)
}

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

#[allow(clippy::too_many_lines)]
pub(crate) fn get_default_builtins(
    options: &crate::CreateOptions,
) -> HashMap<String, BuiltinRegistration> {
    let mut m = HashMap::<String, BuiltinRegistration>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    m.insert("break".into(), special_builtin::<break_::BreakCommand>());
    m.insert(":".into(), special_builtin::<colon::ColonCommand>());
    m.insert(
        "continue".into(),
        special_builtin::<continue_::ContinueCommand>(),
    );
    m.insert(".".into(), special_builtin::<dot::DotCommand>());
    m.insert("eval".into(), special_builtin::<eval::EvalCommand>());
    #[cfg(unix)]
    m.insert("exec".into(), special_builtin::<exec::ExecCommand>());
    m.insert("exit".into(), special_builtin::<exit::ExitCommand>());
    m.insert(
        "export".into(),
        special_decl_builtin::<export::ExportCommand>(),
    );
    m.insert("return".into(), special_builtin::<return_::ReturnCommand>());
    m.insert("set".into(), special_builtin::<set::SetCommand>());
    m.insert("shift".into(), special_builtin::<shift::ShiftCommand>());
    m.insert("trap".into(), special_builtin::<trap::TrapCommand>());
    m.insert("unset".into(), special_builtin::<unset::UnsetCommand>());

    m.insert(
        "readonly".into(),
        special_decl_builtin::<declare::DeclareCommand>(),
    );
    m.insert(
        "times".into(),
        special_builtin::<unimp::UnimplementedCommand>(),
    );

    //
    // Non-special builtins
    //

    m.insert("alias".into(), builtin::<alias::AliasCommand>()); // TODO: should be exec_declaration_builtin
    m.insert("bg".into(), builtin::<bg::BgCommand>());
    m.insert("cd".into(), builtin::<cd::CdCommand>());
    m.insert("false".into(), builtin::<false_::FalseCommand>());
    m.insert("fg".into(), builtin::<fg::FgCommand>());
    m.insert("getopts".into(), builtin::<getopts::GetOptsCommand>());
    m.insert("help".into(), builtin::<help::HelpCommand>());
    m.insert("jobs".into(), builtin::<jobs::JobsCommand>());
    #[cfg(unix)]
    m.insert("kill".into(), builtin::<kill::KillCommand>());
    m.insert("local".into(), decl_builtin::<declare::DeclareCommand>());
    m.insert("pwd".into(), builtin::<pwd::PwdCommand>());
    m.insert("read".into(), builtin::<read::ReadCommand>());
    m.insert("true".into(), builtin::<true_::TrueCommand>());
    m.insert("type".into(), builtin::<type_::TypeCommand>());
    m.insert("umask".into(), builtin::<umask::UmaskCommand>());
    m.insert("unalias".into(), builtin::<unalias::UnaliasCommand>());
    m.insert("wait".into(), builtin::<wait::WaitCommand>());

    // TODO: Unimplemented non-special builtins
    m.insert("command".into(), builtin::<unimp::UnimplementedCommand>());
    m.insert("fc".into(), builtin::<unimp::UnimplementedCommand>());
    m.insert("hash".into(), builtin::<unimp::UnimplementedCommand>());
    m.insert("ulimit".into(), builtin::<unimp::UnimplementedCommand>());

    if !options.sh_mode {
        m.insert("builtin".into(), builtin::<builtin_::BuiltiCommand>());
        m.insert("declare".into(), decl_builtin::<declare::DeclareCommand>());
        m.insert("echo".into(), builtin::<echo::EchoCommand>());
        m.insert("enable".into(), builtin::<enable::EnableCommand>());
        m.insert("let".into(), builtin::<let_::LetCommand>());
        m.insert("printf".into(), builtin::<printf::PrintfCommand>());
        m.insert("shopt".into(), builtin::<shopt::ShoptCommand>());
        m.insert("source".into(), special_builtin::<dot::DotCommand>());
        m.insert("test".into(), builtin::<test::TestCommand>());
        m.insert("[".into(), builtin::<test::TestCommand>());
        m.insert("typeset".into(), builtin::<declare::DeclareCommand>());

        // Completion builtins
        m.insert("complete".into(), builtin::<complete::CompleteCommand>());
        m.insert("compgen".into(), builtin::<complete::CompGenCommand>());
        m.insert("compopt".into(), builtin::<complete::CompOptCommand>());

        // Dir stack builtins
        m.insert("dirs".into(), builtin::<dirs::DirsCommand>());
        m.insert("popd".into(), builtin::<popd::PopdCommand>());
        m.insert("pushd".into(), builtin::<pushd::PushdCommand>());

        // TODO: Unimplemented builtins
        m.insert("bind".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("caller".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("disown".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("history".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("logout".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("mapfile".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("readarray".into(), builtin::<unimp::UnimplementedCommand>());
        m.insert("suspend".into(), builtin::<unimp::UnimplementedCommand>());
    }

    m
}
