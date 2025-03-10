use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rand::Rng;

use crate::arithmetic::Evaluatable;
use crate::env::{EnvironmentLookup, EnvironmentScope, ShellEnvironment};
use crate::interp::{self, Execute, ExecutionParameters, ExecutionResult};
use crate::options::RuntimeOptions;
use crate::sys::fs::PathExt;
use crate::variables::{self, ShellValue, ShellVariable};
use crate::{
    builtins, commands, completion, env, error, expansion, functions, jobs, keywords, openfiles,
    patterns, prompt, sys::users, traps,
};
use crate::{pathcache, sys, trace_categories};

const BASH_MAJOR: u32 = 5;
const BASH_MINOR: u32 = 2;
const BASH_PATCH: u32 = 15;
const BASH_BUILD: u32 = 1;
const BASH_RELEASE: &str = "release";
const BASH_MACHINE: &str = "unknown";

/// Represents an instance of a shell.
pub struct Shell {
    //
    // Core state required by specification
    /// Trap handler configuration for the shell.
    pub traps: traps::TrapHandlerConfig,
    /// Manages files opened and accessible via redirection operators.
    open_files: openfiles::OpenFiles,
    /// The current working directory.
    pub working_dir: PathBuf,
    /// The shell environment, containing shell variables.
    pub env: ShellEnvironment,
    /// Shell function definitions.
    pub funcs: functions::FunctionEnv,
    /// Runtime shell options.
    pub options: RuntimeOptions,
    /// State of managed jobs.
    pub jobs: jobs::JobManager,
    /// Shell aliases.
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    /// The status of the last completed command.
    pub last_exit_status: u8,

    /// The status of each of the commands in the last pipeline.
    pub last_pipeline_statuses: Vec<u8>,

    /// Clone depth from the original ancestor shell.
    depth: usize,

    /// Shell name (a.k.a. $0)
    pub shell_name: Option<String>,

    /// Positional parameters stack ($1 and beyond)
    pub positional_parameters: Vec<String>,

    /// Detailed display string for the shell
    pub shell_product_display_str: Option<String>,

    /// Script call stack.
    script_call_stack: VecDeque<(ScriptCallType, String)>,

    /// Function call stack.
    function_call_stack: VecDeque<FunctionCall>,

    /// Directory stack used by pushd et al.
    pub directory_stack: Vec<PathBuf>,

    /// Current line number being processed.
    current_line_number: u32,

    /// Completion configuration.
    pub completion_config: completion::Config,

    /// Shell built-in commands.
    pub builtins: HashMap<String, builtins::Registration>,

    /// Shell program location cache.
    pub program_location_cache: pathcache::PathCache,

    /// Last "SECONDS" captured time.
    last_stopwatch_time: std::time::SystemTime,

    /// Last "SECONDS" offset requested.
    last_stopwatch_offset: u32,
}

impl Clone for Shell {
    fn clone(&self) -> Self {
        Self {
            traps: self.traps.clone(),
            open_files: self.open_files.clone(),
            working_dir: self.working_dir.clone(),
            env: self.env.clone(),
            funcs: self.funcs.clone(),
            options: self.options.clone(),
            jobs: jobs::JobManager::new(),
            aliases: self.aliases.clone(),
            last_exit_status: self.last_exit_status,
            last_pipeline_statuses: self.last_pipeline_statuses.clone(),
            positional_parameters: self.positional_parameters.clone(),
            shell_name: self.shell_name.clone(),
            shell_product_display_str: self.shell_product_display_str.clone(),
            function_call_stack: self.function_call_stack.clone(),
            script_call_stack: self.script_call_stack.clone(),
            directory_stack: self.directory_stack.clone(),
            current_line_number: self.current_line_number,
            completion_config: self.completion_config.clone(),
            builtins: self.builtins.clone(),
            program_location_cache: self.program_location_cache.clone(),
            last_stopwatch_time: self.last_stopwatch_time,
            last_stopwatch_offset: self.last_stopwatch_offset,
            depth: self.depth + 1,
        }
    }
}

impl AsRef<Shell> for Shell {
    fn as_ref(&self) -> &Shell {
        self
    }
}

impl AsMut<Shell> for Shell {
    fn as_mut(&mut self) -> &mut Shell {
        self
    }
}

/// Options for creating a new shell.
#[derive(Debug, Default)]
pub struct CreateOptions {
    /// Disabled shopt options.
    pub disabled_shopt_options: Vec<String>,
    /// Enabled shopt options.
    pub enabled_shopt_options: Vec<String>,
    /// Disallow overwriting regular files via output redirection.
    pub disallow_overwriting_regular_files_via_output_redirection: bool,
    /// Do not execute commands.
    pub do_not_execute_commands: bool,
    /// Exit after one command.
    pub exit_after_one_command: bool,
    /// Whether the shell is interactive.
    pub interactive: bool,
    /// Whether the shell is a login shell.
    pub login: bool,
    /// Whether to skip using a readline-like interface for input.
    pub no_editing: bool,
    /// Whether to skip sourcing the system profile.
    pub no_profile: bool,
    /// Whether to skip sourcing the user's rc file.
    pub no_rc: bool,
    /// Whether to skip inheriting environment variables from the calling process.
    pub do_not_inherit_env: bool,
    /// Whether the shell is in POSIX compliance mode.
    pub posix: bool,
    /// Whether to print commands and arguments as they are read.
    pub print_commands_and_arguments: bool,
    /// Whether commands are being read from stdin.
    pub read_commands_from_stdin: bool,
    /// The name of the shell.
    pub shell_name: Option<String>,
    /// Optionally provides a display string describing the version and variant of the shell.
    pub shell_product_display_str: Option<String>,
    /// Whether to run in maximal POSIX sh compatibility mode.
    pub sh_mode: bool,
    /// Whether to print verbose output.
    pub verbose: bool,
    /// Maximum function call depth.
    pub max_function_call_depth: Option<usize>,
}

/// Represents an executing script.
#[derive(Clone, Debug)]
pub enum ScriptCallType {
    /// The script was sourced.
    Sourced,
    /// The script was executed.
    Executed,
}

/// Represents an active shell function call.
#[derive(Clone, Debug)]
pub struct FunctionCall {
    /// The name of the function invoked.
    function_name: String,
    /// The definition of the invoked function.
    function_definition: Arc<brush_parser::ast::FunctionDefinition>,
}

impl Shell {
    /// Returns a new shell instance created with the given options.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use when creating the shell.
    pub async fn new(options: &CreateOptions) -> Result<Shell, error::Error> {
        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            traps: traps::TrapHandlerConfig::default(),
            open_files: openfiles::OpenFiles::default(),
            working_dir: std::env::current_dir()?,
            env: env::ShellEnvironment::new(),
            funcs: functions::FunctionEnv::default(),
            options: RuntimeOptions::defaults_from(options),
            jobs: jobs::JobManager::new(),
            aliases: HashMap::default(),
            last_exit_status: 0,
            last_pipeline_statuses: vec![0],
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
            shell_product_display_str: options.shell_product_display_str.clone(),
            function_call_stack: VecDeque::new(),
            script_call_stack: VecDeque::new(),
            directory_stack: vec![],
            current_line_number: 0,
            completion_config: completion::Config::default(),
            builtins: builtins::get_default_builtins(options),
            program_location_cache: pathcache::PathCache::default(),
            last_stopwatch_time: std::time::SystemTime::now(),
            last_stopwatch_offset: 0,
            depth: 0,
        };

        // TODO: Without this a script that sets extglob will fail because we
        // parse the entire script with the same settings.
        shell.options.extended_globbing = true;

        // Initialize environment.
        shell.initialize_vars(options)?;

        // Load profiles/configuration.
        shell.load_config(options).await?;

        Ok(shell)
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unwrap_in_result)]
    fn initialize_vars(&mut self, options: &CreateOptions) -> Result<(), error::Error> {
        // Seed parameters from environment (unless requested not to do so).
        if !options.do_not_inherit_env {
            for (k, v) in std::env::vars() {
                let mut var = ShellVariable::new(ShellValue::String(v));
                var.export();
                self.env.set_global(k, var)?;
            }
        }

        // TODO(vars): implement $_

        // BASH
        if let Some(shell_name) = &options.shell_name {
            self.env
                .set_global("BASH", ShellVariable::new(shell_name.into()))?;
        }

        // BASHOPTS
        let mut bashopts_var = ShellVariable::new(ShellValue::Dynamic {
            getter: |shell| shell.options.get_shopt_optstr().into(),
            setter: |_| (),
        });
        bashopts_var.set_readonly();
        self.env.set_global("BASHOPTS", bashopts_var)?;

        // BASHPID
        let mut bashpid_var =
            ShellVariable::new(ShellValue::String(std::process::id().to_string()));
        bashpid_var.treat_as_integer();
        self.env.set_global("BASHPID", bashpid_var)?;

        // BASH_ALIASES
        self.env.set_global(
            "BASH_ALIASES",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| {
                    let values = variables::ArrayLiteral(
                        shell
                            .aliases
                            .iter()
                            .map(|(k, v)| (Some(k.to_owned()), v.to_owned()))
                            .collect::<Vec<_>>(),
                    );

                    ShellValue::associative_array_from_literals(values).unwrap()
                },
                setter: |_| (),
            }),
        )?;

        // TODO(vars): when extdebug is enabled, BASH_ARGC and BASH_ARGV are set to valid values
        // TODO(vars): implement BASH_ARGC
        // TODO(vars): implement BASH_ARGV

        // BASH_ARGV0
        self.env.set_global(
            "BASH_ARGV0",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| {
                    let argv0 = shell.shell_name.as_deref().unwrap_or_default();
                    argv0.to_string().into()
                },
                // TODO(vars): implement updating BASH_ARGV0
                setter: |_| (),
            }),
        )?;

        // TODO(vars): implement mutation of BASH_CMDS
        self.env.set_global(
            "BASH_CMDS",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| shell.program_location_cache.to_value().unwrap(),
                setter: |_| (),
            }),
        )?;

        // TODO(vars): implement BASH_COMMAND
        // TODO(vars): implement BASH_EXECUTIION_STRING
        // TODO(vars): implement BASH_LINENO

        // BASH_SOURCE
        self.env.set_global(
            "BASH_SOURCE",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| shell.get_bash_source_value(),
                setter: |_| (),
            }),
        )?;

        // BASH_SUBSHELL
        self.env.set_global(
            "BASH_SUBSHELL",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| shell.depth.to_string().into(),
                setter: |_| (),
            }),
        )?;

        // BASH_VERSINFO
        let mut bash_versinfo_var = ShellVariable::new(ShellValue::indexed_array_from_strs(
            [
                BASH_MAJOR.to_string().as_str(),
                BASH_MINOR.to_string().as_str(),
                BASH_PATCH.to_string().as_str(),
                BASH_BUILD.to_string().as_str(),
                BASH_RELEASE,
                BASH_MACHINE,
            ]
            .as_slice(),
        ));
        bash_versinfo_var.set_readonly();
        self.env.set_global("BASH_VERSINFO", bash_versinfo_var)?;

        // BASH_VERSION
        self.env.set_global(
            "BASH_VERSION",
            ShellVariable::new(
                std::format!("{BASH_MAJOR}.{BASH_MINOR}.{BASH_PATCH}({BASH_BUILD})-{BASH_RELEASE}")
                    .into(),
            ),
        )?;

        // COMP_WORDBREAKS
        self.env.set_global(
            "COMP_WORDBREAKS",
            ShellVariable::new(" \t\n\"\'@><=;|&(:".into()),
        )?;

        // DIRSTACK
        self.env.set_global(
            "DIRSTACK",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| {
                    shell
                        .directory_stack
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .into()
                },
                setter: |_| (),
            }),
        )?;

        // EPOCHREALTIME
        self.env.set_global(
            "EPOCHREALTIME",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |_shell| {
                    let now = std::time::SystemTime::now();
                    let since_epoch = now
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    since_epoch.as_secs_f64().to_string().into()
                },
                setter: |_| (),
            }),
        )?;

        // EPOCHSECONDS
        self.env.set_global(
            "EPOCHSECONDS",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |_shell| {
                    let now = std::time::SystemTime::now();
                    let since_epoch = now
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    since_epoch.as_secs().to_string().into()
                },
                setter: |_| (),
            }),
        )?;

        // EUID
        #[cfg(unix)]
        {
            let mut euid_var = ShellVariable::new(ShellValue::String(format!(
                "{}",
                uzers::get_effective_uid()
            )));
            euid_var.treat_as_integer().set_readonly();
            self.env.set_global("EUID", euid_var)?;
        }

        // FUNCNAME
        self.env.set_global(
            "FUNCNAME",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| shell.get_funcname_value(),
                setter: |_| (),
            }),
        )?;

        // GROUPS
        // N.B. We could compute this up front, but we choose to make it dynamic so that we
        // don't have to make costly system calls if the user never accesses it.
        self.env.set_global(
            "GROUPS",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |_shell| {
                    let groups = sys::users::get_user_group_ids().unwrap_or_default();
                    ShellValue::indexed_array_from_strings(
                        groups.into_iter().map(|gid| gid.to_string()),
                    )
                },
                setter: |_| (),
            }),
        )?;

        // TODO(vars): implement HISTCMD

        // HISTFILE (if not already set)
        if !self.env.is_set("HISTFILE") {
            if let Some(home_dir) = self.get_home_dir() {
                let histfile = home_dir.join(".brush_history");
                self.env.set_global(
                    "HISTFILE",
                    ShellVariable::new(ShellValue::String(histfile.to_string_lossy().to_string())),
                )?;
            }
        }

        // HOSTNAME
        self.env.set_global(
            "HOSTNAME",
            ShellVariable::new(
                sys::network::get_hostname()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
                    .into(),
            ),
        )?;

        // HOSTTYPE
        #[cfg(unix)]
        {
            if let Ok(info) = nix::sys::utsname::uname() {
                self.env.set_global(
                    "HOSTTYPE",
                    ShellVariable::new(info.machine().to_string_lossy().to_string().into()),
                )?;
            }
        }

        // IFS
        self.env
            .set_global("IFS", ShellVariable::new(" \t\n".into()))?;

        // LINENO
        self.env.set_global(
            "LINENO",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| shell.current_line_number.to_string().into(),
                setter: |_| (),
            }),
        )?;

        // MACHTYPE
        self.env
            .set_global("MACHTYPE", ShellVariable::new(BASH_MACHINE.into()))?;

        // OLDPWD (initialization)
        if !self.env.is_set("OLDPWD") {
            let mut oldpwd_var =
                ShellVariable::new(ShellValue::Unset(variables::ShellValueUnsetType::Untyped));
            oldpwd_var.export();
            self.env.set_global("OLDPWD", oldpwd_var)?;
        }

        // OPTERR
        self.env
            .set_global("OPTERR", ShellVariable::new("1".into()))?;

        // OPTIND
        let mut optind_var = ShellVariable::new("1".into());
        optind_var.treat_as_integer();
        self.env.set_global("OPTIND", optind_var)?;

        // OSTYPE
        let os_type = match std::env::consts::OS {
            "linux" => "linux-gnu",
            "windows" => "windows",
            _ => "unknown",
        };
        self.env
            .set_global("OSTYPE", ShellVariable::new(os_type.into()))?;

        // PATH (if not already set)
        #[cfg(unix)]
        if !self.env.is_set("PATH") {
            let default_path_str = sys::fs::get_default_executable_search_paths().join(":");
            self.env
                .set_global("PATH", ShellVariable::new(default_path_str.into()))?;
        }

        // PIPESTATUS
        // TODO: Investigate what happens if this gets unset.
        // TODO: Investigate if this needs to be saved/preserved across prompt display.
        self.env.set_global(
            "PIPESTATUS",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| {
                    ShellValue::indexed_array_from_strings(
                        shell.last_pipeline_statuses.iter().map(|s| s.to_string()),
                    )
                },
                setter: |_| (),
            }),
        )?;

        // PPID
        if let Some(ppid) = sys::terminal::get_parent_process_id() {
            let mut ppid_var = ShellVariable::new(ppid.to_string().into());
            ppid_var.treat_as_integer().set_readonly();
            self.env.set_global("PPID", ppid_var)?;
        }

        // RANDOM
        let mut random_var = ShellVariable::new(ShellValue::Dynamic {
            getter: get_random_value,
            setter: |_| (),
        });
        random_var.treat_as_integer();
        self.env.set_global("RANDOM", random_var)?;

        // SECONDS
        self.env.set_global(
            "SECONDS",
            ShellVariable::new(ShellValue::Dynamic {
                getter: |shell| {
                    let now = std::time::SystemTime::now();
                    let since_last = now
                        .duration_since(shell.last_stopwatch_time)
                        .unwrap_or_default();
                    let total_seconds =
                        since_last.as_secs() + u64::from(shell.last_stopwatch_offset);
                    total_seconds.to_string().into()
                },
                // TODO(vars): implement updating SECONDS
                setter: |_| (),
            }),
        )?;

        // SHELL
        if let Ok(exe_path) = std::env::current_exe() {
            self.env.set_global(
                "SHELL",
                ShellVariable::new(exe_path.to_string_lossy().to_string().into()),
            )?;
        }

        // SHELLOPTS
        let mut shellopts_var = ShellVariable::new(ShellValue::Dynamic {
            getter: |shell| shell.options.get_set_o_optstr().into(),
            setter: |_| (),
        });
        shellopts_var.set_readonly();
        self.env.set_global("SHELLOPTS", shellopts_var)?;

        // SHLVL
        let input_shlvl = self.get_env_str("SHLVL").unwrap_or("0".into());
        let updated_shlvl = input_shlvl.as_ref().parse::<u32>().unwrap_or(0) + 1;
        let mut shlvl_var = ShellVariable::new(updated_shlvl.to_string().into());
        shlvl_var.export();
        self.env.set_global("SHLVL", shlvl_var)?;

        // SRANDOM
        let mut random_var = ShellVariable::new(ShellValue::Dynamic {
            getter: get_srandom_value,
            setter: |_| (),
        });
        random_var.treat_as_integer();
        self.env.set_global("SRANDOM", random_var)?;

        // PS1 / PS2
        if options.interactive {
            if !self.env.is_set("PS1") {
                self.env
                    .set_global("PS1", ShellVariable::new(r"\s-\v\$ ".into()))?;
            }

            if !self.env.is_set("PS2") {
                self.env
                    .set_global("PS2", ShellVariable::new("> ".into()))?;
            }
        }

        // PS4
        if !self.env.is_set("PS4") {
            self.env
                .set_global("PS4", ShellVariable::new("+ ".into()))?;
        }

        //
        // PWD
        //
        // Reflect our actual working directory. There's a chance
        // we inherited an out-of-sync version of the variable. Future updates
        // will be handled by set_working_dir().
        //
        let pwd = self.working_dir.to_string_lossy().to_string();
        let mut pwd_var = ShellVariable::new(pwd.into());
        pwd_var.export();
        self.env.set_global("PWD", pwd_var)?;

        // UID
        #[cfg(unix)]
        {
            let mut uid_var =
                ShellVariable::new(ShellValue::String(format!("{}", uzers::get_current_uid())));
            uid_var.treat_as_integer().set_readonly();
            self.env.set_global("UID", uid_var)?;
        }

        Ok(())
    }

    async fn load_config(&mut self, options: &CreateOptions) -> Result<(), error::Error> {
        let mut params = self.default_exec_params();
        params.process_group_policy = interp::ProcessGroupPolicy::SameProcessGroup;

        if options.login {
            // --noprofile means skip this.
            if options.no_profile {
                return Ok(());
            }

            //
            // Source /etc/profile if it exists.
            //
            // Next source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            //
            self.source_if_exists(Path::new("/etc/profile"), &params)
                .await?;
            if let Some(home_path) = self.get_home_dir() {
                if options.sh_mode {
                    self.source_if_exists(home_path.join(".profile").as_path(), &params)
                        .await?;
                } else {
                    if !self
                        .source_if_exists(home_path.join(".bash_profile").as_path(), &params)
                        .await?
                    {
                        if !self
                            .source_if_exists(home_path.join(".bash_login").as_path(), &params)
                            .await?
                        {
                            self.source_if_exists(home_path.join(".profile").as_path(), &params)
                                .await?;
                        }
                    }
                }
            }
        } else {
            if options.interactive {
                // --norc means skip this. Also skip in sh mode.
                if options.no_rc || options.sh_mode {
                    return Ok(());
                }

                //
                // For non-login interactive shells, load in this order:
                //
                //     /etc/bash.bashrc
                //     ~/.bashrc
                //
                self.source_if_exists(Path::new("/etc/bash.bashrc"), &params)
                    .await?;
                if let Some(home_path) = self.get_home_dir() {
                    self.source_if_exists(home_path.join(".bashrc").as_path(), &params)
                        .await?;
                    self.source_if_exists(home_path.join(".brushrc").as_path(), &params)
                        .await?;
                }
            } else {
                let env_var_name = if options.sh_mode { "ENV" } else { "BASH_ENV" };

                if self.env.is_set(env_var_name) {
                    //
                    // TODO: look at $ENV/BASH_ENV; source its expansion if that file exists
                    //
                    return error::unimp(
                        "load config from $ENV/BASH_ENV for non-interactive, non-login shell",
                    );
                }
            }
        }

        Ok(())
    }

    async fn source_if_exists(
        &mut self,
        path: &Path,
        params: &ExecutionParameters,
    ) -> Result<bool, error::Error> {
        if path.exists() {
            self.source_script(path, std::iter::empty::<String>(), params)
                .await?;
            Ok(true)
        } else {
            tracing::debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    pub async fn source_script<S: AsRef<str>, I: Iterator<Item = S>>(
        &mut self,
        path: &Path,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        self.parse_and_execute_script_file(path, args, params, ScriptCallType::Sourced)
            .await
    }

    /// Parse and execute the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn parse_and_execute_script_file<S: AsRef<str>, I: Iterator<Item = S>>(
        &mut self,
        path: &Path,
        args: I,
        params: &ExecutionParameters,
        call_type: ScriptCallType,
    ) -> Result<ExecutionResult, error::Error> {
        tracing::debug!("sourcing: {}", path.display());
        let opened_file: openfiles::OpenFile = self
            .open_file(path, params)
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e.into()))?;

        if opened_file.is_dir() {
            return Err(error::Error::FailedSourcingFile(
                path.to_owned(),
                error::Error::IsADirectory.into(),
            ));
        }

        let source_info = brush_parser::SourceInfo {
            source: path.to_string_lossy().to_string(),
        };

        self.source_file(opened_file, &source_info, args, params, call_type)
            .await
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `file` - The file to source.
    /// * `source_info` - Information about the source of the script.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn source_file<F: Read, S: AsRef<str>, I: Iterator<Item = S>>(
        &mut self,
        file: F,
        source_info: &brush_parser::SourceInfo,
        args: I,
        params: &ExecutionParameters,
        call_type: ScriptCallType,
    ) -> Result<ExecutionResult, error::Error> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser =
            brush_parser::Parser::new(&mut reader, &self.parser_options(), source_info);

        tracing::debug!(target: trace_categories::PARSE, "Parsing sourced file: {}", source_info.source);
        let parse_result = parser.parse();

        let mut other_positional_parameters = args.map(|s| s.as_ref().to_owned()).collect();
        let mut other_shell_name = Some(source_info.source.clone());

        // TODO: Find a cleaner way to change args.
        std::mem::swap(&mut self.shell_name, &mut other_shell_name);
        std::mem::swap(
            &mut self.positional_parameters,
            &mut other_positional_parameters,
        );

        self.script_call_stack
            .push_front((call_type.clone(), source_info.source.clone()));

        let result = self
            .run_parsed_result(parse_result, source_info, params)
            .await;

        self.script_call_stack.pop_front();

        // Restore.
        std::mem::swap(&mut self.shell_name, &mut other_shell_name);
        std::mem::swap(
            &mut self.positional_parameters,
            &mut other_positional_parameters,
        );

        result
    }

    /// Invokes a function defined in this shell, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to invoke.
    /// * `args` - The arguments to pass to the function.
    pub async fn invoke_function(&mut self, name: &str, args: &[&str]) -> Result<u8, error::Error> {
        // TODO: Figure out if *all* callers have the same process group policy.
        let params = self.default_exec_params();

        let command_name = String::from(name);

        let func_registration = self
            .funcs
            .get(name)
            .ok_or_else(|| error::Error::FunctionNotFound(name.to_owned()))?;

        let func = func_registration.definition.clone();

        let context = commands::ExecutionContext {
            shell: self,
            command_name,
            params,
        };

        let command_args = args
            .iter()
            .map(|s| commands::CommandArg::String(String::from(*s)))
            .collect::<Vec<_>>();

        match commands::invoke_shell_function(func, context, &command_args).await? {
            commands::CommandSpawnResult::SpawnedProcess(_) => {
                error::unimp("child spawned from function invocation")
            }
            commands::CommandSpawnResult::ImmediateExit(code) => Ok(code),
            commands::CommandSpawnResult::ExitShell(code) => Ok(code),
            commands::CommandSpawnResult::ReturnFromFunctionOrScript(code) => Ok(code),
            commands::CommandSpawnResult::BreakLoop(_)
            | commands::CommandSpawnResult::ContinueLoop(_) => {
                error::unimp("break or continue returned from function invocation")
            }
        }
    }

    /// Executes the given string as a shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute.
    /// * `params` - Execution parameters.
    pub async fn run_string(
        &mut self,
        command: String,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // TODO: Actually track line numbers; this is something of a hack, assuming each time
        // this function is invoked we are on the next line of the input. For one thing,
        // each string we run could be multiple lines.
        self.current_line_number += 1;

        let parse_result = self.parse_string(command);
        let source_info = brush_parser::SourceInfo {
            source: String::from("main"),
        };
        self.run_parsed_result(parse_result, &source_info, params)
            .await
    }

    /// Parses the given string as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse as a program.
    pub fn parse_string(
        &self,
        s: String,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
        parse_string_impl(s, self.parser_options())
    }

    /// Applies basic shell expansion to the provided string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand.
    pub async fn basic_expand_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<String, error::Error> {
        let result = expansion::basic_expand_str(self, params, s.as_ref()).await?;
        Ok(result)
    }

    /// Applies full shell expansion and field splitting to the provided string; returns
    /// a sequence of fields.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand and split.
    pub async fn full_expand_and_split_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<Vec<String>, error::Error> {
        let result = expansion::full_expand_and_split_str(self, params, s.as_ref()).await?;
        Ok(result)
    }

    /// Returns the default execution parameters for this shell.
    pub fn default_exec_params(&self) -> ExecutionParameters {
        ExecutionParameters {
            open_files: self.open_files.clone(),
            ..Default::default()
        }
    }

    /// Executes the given script file, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `script_path` - The path to the script file to execute.
    /// * `args` - The arguments to pass to the script as positional parameters.
    pub async fn run_script<S: AsRef<str>, I: Iterator<Item = S>>(
        &mut self,
        script_path: &Path,
        args: I,
    ) -> Result<ExecutionResult, error::Error> {
        let params = self.default_exec_params();
        self.parse_and_execute_script_file(script_path, args, &params, ScriptCallType::Executed)
            .await
    }

    async fn run_parsed_result(
        &mut self,
        parse_result: Result<brush_parser::ast::Program, brush_parser::ParseError>,
        source_info: &brush_parser::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut error_prefix = String::new();

        if !source_info.source.is_empty() {
            error_prefix = format!("{}: ", source_info.source);
        }

        let result = match parse_result {
            Ok(prog) => match self.run_program(prog, params).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("error: {:#}", e);
                    self.last_exit_status = 1;
                    ExecutionResult::new(1)
                }
            },
            Err(brush_parser::ParseError::ParsingNearToken(token_near_error)) => {
                let error_loc = &token_near_error.location().start;

                tracing::error!(
                    "{}syntax error near token `{}' (line {} col {})",
                    error_prefix,
                    token_near_error.to_str(),
                    error_loc.line,
                    error_loc.column,
                );
                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => {
                tracing::error!("{}syntax error at end of input", error_prefix);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(brush_parser::ParseError::Tokenizing { inner, position }) => {
                let mut error_message = error_prefix.clone();
                error_message.push_str(inner.to_string().as_str());

                if let Some(position) = position {
                    write!(
                        error_message,
                        " (detected near line {} column {})",
                        position.line, position.column
                    )?;
                }

                tracing::error!("{}", error_message);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
        };

        Ok(result)
    }

    /// Executes the given parsed shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `program` - The program to execute.
    /// * `params` - Execution parameters.
    pub async fn run_program(
        &mut self,
        program: brush_parser::ast::Program,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        program.execute(self, params).await
    }

    fn default_prompt(&self) -> &'static str {
        if self.options.sh_mode {
            "$ "
        } else {
            "brush$ "
        }
    }

    /// Composes the shell's post-input, pre-command prompt, applying all appropriate expansions.
    pub async fn compose_precmd_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS0", "").await
    }

    /// Composes the shell's prompt, applying all appropriate expansions.
    pub async fn compose_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS1", self.default_prompt()).await
    }

    /// Compose's the shell's alternate-side prompt, applying all appropriate expansions.
    #[allow(clippy::unused_async)]
    pub async fn compose_alt_side_prompt(&mut self) -> Result<String, error::Error> {
        // This is a brush extension.
        self.expand_prompt_var("BRUSH_PS_ALT", "").await
    }

    /// Composes the shell's continuation prompt.
    pub async fn compose_continuation_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS2", "> ").await
    }

    async fn expand_prompt_var(
        &mut self,
        var_name: &str,
        default: &str,
    ) -> Result<String, error::Error> {
        //
        // TODO(prompt): bash appears to do this in a subshell; we need to investigate
        // if that's required.
        //

        // Retrieve the spec.
        let prompt_spec = self.parameter_or_default(var_name, default);
        if prompt_spec.is_empty() {
            return Ok(String::new());
        }

        // Expand it.
        let formatted_prompt = prompt::expand_prompt(self, prompt_spec.into_owned())?;

        // Now expand.
        let params = self.default_exec_params();
        expansion::basic_expand_str(self, &params, &formatted_prompt).await
    }

    /// Returns the exit status of the last command executed in this shell.
    pub fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    fn parameter_or_default<'a>(&'a self, name: &str, default: &'a str) -> Cow<'a, str> {
        self.get_env_str(name).unwrap_or(default.into())
    }

    /// Returns the options that should be used for parsing shell programs; reflects
    /// the current configuration state of the shell and may change over time.
    pub fn parser_options(&self) -> brush_parser::ParserOptions {
        brush_parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
            sh_mode: self.options.sh_mode,
            tilde_expansion: true,
        }
    }

    /// Returns whether or not the shell is actively executing in a sourced script.
    pub(crate) fn in_sourced_script(&self) -> bool {
        self.script_call_stack
            .front()
            .is_some_and(|(ty, _)| matches!(ty, ScriptCallType::Sourced))
    }

    /// Returns whether or not the shell is actively executing in a shell function.
    pub(crate) fn in_function(&self) -> bool {
        !self.function_call_stack.is_empty()
    }

    /// Updates the shell's internal tracking state to reflect that a new shell
    /// function is being entered.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being entered.
    /// * `function_def` - The definition of the function being entered.
    pub(crate) fn enter_function(
        &mut self,
        name: &str,
        function_def: &Arc<brush_parser::ast::FunctionDefinition>,
    ) -> Result<(), error::Error> {
        if let Some(max_call_depth) = self.options.max_function_call_depth {
            if self.function_call_stack.len() >= max_call_depth {
                return Err(error::Error::MaxFunctionCallDepthExceeded);
            }
        }

        if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
            let depth = self.function_call_stack.len();
            let prefix = repeated_char_str(' ', depth);
            tracing::debug!(target: trace_categories::FUNCTIONS, "Entering func [depth={depth}]: {prefix}{name}");
        }

        self.function_call_stack.push_front(FunctionCall {
            function_name: name.to_owned(),
            function_definition: function_def.clone(),
        });
        self.env.push_scope(env::EnvironmentScope::Local);
        Ok(())
    }

    /// Updates the shell's internal tracking state to reflect that the shell
    /// has exited the top-most function on its call stack.
    pub(crate) fn leave_function(&mut self) -> Result<(), error::Error> {
        self.env.pop_scope(env::EnvironmentScope::Local)?;

        if let Some(exited_call) = self.function_call_stack.pop_front() {
            if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
                let depth = self.function_call_stack.len();
                let prefix = repeated_char_str(' ', depth);
                tracing::debug!(target: trace_categories::FUNCTIONS, "Exiting func  [depth={depth}]: {prefix}{}", exited_call.function_name);
            }
        }

        Ok(())
    }

    fn get_funcname_value(&self) -> variables::ShellValue {
        if self.function_call_stack.is_empty() {
            ShellValue::Unset(variables::ShellValueUnsetType::IndexedArray)
        } else {
            self.function_call_stack
                .iter()
                .map(|s| s.function_name.as_str())
                .collect::<Vec<_>>()
                .into()
        }
    }

    fn get_bash_source_value(&self) -> variables::ShellValue {
        if self.function_call_stack.is_empty() {
            self.script_call_stack
                .front()
                .map_or_else(Vec::new, |(_call_type, s)| vec![s.as_ref()])
                .into()
        } else {
            self.function_call_stack
                .iter()
                .map(|s| s.function_definition.source.as_ref())
                .collect::<Vec<_>>()
                .into()
        }
    }

    /// Returns the path to the history file used by the shell, if one is set.
    pub fn get_history_file_path(&self) -> Option<PathBuf> {
        self.get_env_str("HISTFILE")
            .map(|s| PathBuf::from(s.into_owned()))
    }

    /// Returns the number of the line being executed in the currently executing program.
    pub(crate) fn get_current_input_line_number(&self) -> u32 {
        self.current_line_number
    }

    /// Tries to retrieve a variable from the shell's environment, converting it into its
    /// string form.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get_env_str(&self, name: &str) -> Option<Cow<'_, str>> {
        self.env.get_str(name, self)
    }

    /// Returns the current value of the IFS variable, or the default value if it is not set.
    pub(crate) fn get_ifs(&self) -> Cow<'_, str> {
        self.get_env_str("IFS").unwrap_or_else(|| " \t\n".into())
    }

    /// Returns the first character of the IFS variable, or a space if it is not set.
    pub(crate) fn get_ifs_first_char(&self) -> char {
        self.get_ifs().chars().next().unwrap_or(' ')
    }

    /// Generates command completions for the shell.
    ///
    /// # Arguments
    ///
    /// * `input` - The input string to generate completions for.
    /// * `position` - The position in the input string to generate completions at.
    pub async fn get_completions(
        &mut self,
        input: &str,
        position: usize,
    ) -> Result<completion::Completions, error::Error> {
        let completion_config = self.completion_config.clone();
        completion_config
            .get_completions(self, input, position)
            .await
    }

    /// Finds executables in the shell's current default PATH, matching the given glob pattern.
    ///
    /// # Arguments
    ///
    /// * `required_glob_pattern` - The glob pattern to match against.
    pub fn find_executables_in_path(&self, required_glob_pattern: &str) -> Vec<PathBuf> {
        self.find_executables_in(
            self.env
                .get_str("PATH", self)
                .unwrap_or_default()
                .split(':'),
            required_glob_pattern,
        )
    }

    /// Finds executables in the given paths, matching the given glob pattern.
    ///
    /// # Arguments
    ///
    /// * `paths` - The paths to search in
    /// * `required_glob_pattern` - The glob pattern to match against.
    #[allow(clippy::manual_flatten)]
    pub fn find_executables_in<T: AsRef<str>>(
        &self,
        paths: impl Iterator<Item = T>,
        required_glob_pattern: &str,
    ) -> Vec<PathBuf> {
        let is_executable = |path: &Path| path.is_file() && path.executable();

        let mut executables = vec![];
        for dir_str in paths {
            let dir_str = dir_str.as_ref();
            let pattern =
                patterns::Pattern::from(std::format!("{dir_str}/{required_glob_pattern}"))
                    .set_extended_globbing(self.options.extended_globbing)
                    .set_case_insensitive(self.options.case_insensitive_pathname_expansion);

            // TODO: Pass through quoting.
            if let Ok(entries) = pattern.expand(
                &self.working_dir,
                Some(&is_executable),
                &patterns::FilenameExpansionOptions::default(),
            ) {
                for entry in entries {
                    executables.push(PathBuf::from(entry));
                }
            }
        }

        executables
    }

    /// Determines whether the given filename is the name of an executable in one of the
    /// directories in the shell's current PATH. If found, returns the path.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path<S: AsRef<str>>(
        &self,
        candidate_name: S,
    ) -> Option<PathBuf> {
        for dir_str in self.get_env_str("PATH").unwrap_or_default().split(':') {
            let candidate_path = Path::new(dir_str).join(candidate_name.as_ref());
            if candidate_path.executable() {
                return Some(candidate_path);
            }
        }
        None
    }

    /// Uses the shell's hash-based path cache to check whether the given filename is the name
    /// of an executable in one of the directories in the shell's current PATH. If found,
    /// ensures the path is in the cache and returns it.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path_using_cache<S: AsRef<str>>(
        &mut self,
        candidate_name: S,
    ) -> Option<PathBuf> {
        if let Some(cached_path) = self.program_location_cache.get(&candidate_name) {
            Some(cached_path)
        } else if let Some(found_path) = self.find_first_executable_in_path(&candidate_name) {
            self.program_location_cache
                .set(&candidate_name, found_path.clone());
            Some(found_path)
        } else {
            None
        }
    }

    /// Gets the absolute form of the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to get the absolute form of.
    pub fn get_absolute_path(&self, path: &Path) -> PathBuf {
        if path.as_os_str().is_empty() || path.is_absolute() {
            path.to_owned()
        } else {
            self.working_dir.join(path)
        }
    }

    /// Opens the given file.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to open; may be relative to the shell's working directory.
    /// * `params` - Execution parameters.
    pub(crate) fn open_file(
        &self,
        path: &Path,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, error::Error> {
        let path_to_open = self.get_absolute_path(path);

        // See if this is a reference to a file descriptor, in which case the actual
        // /dev/fd* file path for this process may not match with what's in the execution
        // parameters.
        if let Some(parent) = path_to_open.parent() {
            if parent == Path::new("/dev/fd") {
                if let Some(filename) = path_to_open.file_name() {
                    if let Ok(fd_num) = filename.to_string_lossy().to_string().parse::<u32>() {
                        if let Some(open_file) = params.open_files.files.get(&fd_num) {
                            return open_file.try_dup();
                        }
                    }
                }
            }
        }

        Ok(std::fs::File::open(path_to_open)?.into())
    }

    /// Replaces the shell's file descriptor table with the given one.
    ///
    /// # Arguments
    ///
    /// * `open_files` - The new file descriptor table to use.
    pub(crate) fn replace_open_files(&mut self, open_files: openfiles::OpenFiles) {
        self.open_files = open_files;
    }

    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    pub fn set_working_dir(&mut self, target_dir: &Path) -> Result<(), error::Error> {
        let abs_path = self.get_absolute_path(target_dir);

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::Error::NotADirectory(abs_path));
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // TODO: Don't canonicalize, just normalize.
        let cleaned_path = abs_path.canonicalize()?;

        let pwd = cleaned_path.to_string_lossy().to_string();

        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |var| {
                var.export();
                Ok(())
            },
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;
        let oldpwd = std::mem::replace(&mut self.working_dir, cleaned_path);

        self.env.update_or_add(
            "OLDPWD",
            variables::ShellValueLiteral::Scalar(oldpwd.to_string_lossy().to_string()),
            |var| {
                var.export();
                Ok(())
            },
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    /// Tilde-shortens the given string, replacing the user's home directory with a tilde.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to shorten.
    pub(crate) fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.get_home_dir() {
            if let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref()) {
                return format!("~{stripped}");
            }
        }
        s
    }

    /// Returns the shell's current home directory, if available.
    pub(crate) fn get_home_dir(&self) -> Option<PathBuf> {
        Self::get_home_dir_with_env(&self.env, self)
    }

    fn get_home_dir_with_env(env: &ShellEnvironment, shell: &Shell) -> Option<PathBuf> {
        if let Some(home) = env.get_str("HOME", shell) {
            Some(PathBuf::from(home.to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_current_user_home_dir()
        }
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard output stream using `write!` at al.
    pub fn stdout(&self) -> openfiles::OpenFile {
        self.open_files.files.get(&1).unwrap().try_dup().unwrap()
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard error stream using `write!` et al.
    pub fn stderr(&self) -> openfiles::OpenFile {
        self.open_files.files.get(&2).unwrap().try_dup().unwrap()
    }

    /// Outputs `set -x` style trace output for a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to trace.
    pub(crate) async fn trace_command<S: AsRef<str>>(
        &mut self,
        command: S,
    ) -> Result<(), error::Error> {
        let ps4 = self.as_mut().expand_prompt_var("PS4", "").await?;

        let mut prefix = ps4.to_string();

        let additional_depth = self.script_call_stack.len() + self.depth;
        if let Some(c) = prefix.chars().next() {
            for _ in 0..additional_depth {
                prefix.insert(0, c);
            }
        }

        writeln!(self.stderr(), "{prefix}{}", command.as_ref())?;
        Ok(())
    }

    /// Returns the keywords that are reserved by the shell.
    pub(crate) fn get_keywords(&self) -> Vec<String> {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.iter().cloned().collect()
        } else {
            keywords::KEYWORDS.iter().cloned().collect()
        }
    }

    /// Checks if the given string is a keyword reserved in this shell.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to check.
    pub fn is_keyword(&self, s: &str) -> bool {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.contains(s)
        } else {
            keywords::KEYWORDS.contains(s)
        }
    }

    /// Checks for completed jobs in the shell, reporting any changes found.
    pub fn check_for_completed_jobs(&mut self) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        if self.options.enable_job_control {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }

    /// Evaluate the given arithmetic expression, returning the result.
    pub fn eval_arithmetic(
        &mut self,
        expr: &brush_parser::ast::ArithmeticExpr,
    ) -> Result<i64, error::Error> {
        Ok(expr.eval(self)?)
    }
}

#[cached::proc_macro::cached(size = 64, result = true)]
fn parse_string_impl(
    s: String,
    parser_options: brush_parser::ParserOptions,
) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
    let mut reader = std::io::BufReader::new(s.as_bytes());
    let source_info = brush_parser::SourceInfo {
        source: String::from("main"),
    };
    let mut parser: brush_parser::Parser<&mut std::io::BufReader<&[u8]>> =
        brush_parser::Parser::new(&mut reader, &parser_options, &source_info);

    tracing::debug!(target: trace_categories::PARSE, "Parsing string as program...");
    parser.parse()
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}

fn get_random_value(_shell: &Shell) -> ShellValue {
    let mut rng = rand::rng();
    let num = rng.random_range(0..32768);
    let str = num.to_string();
    str.into()
}

fn get_srandom_value(_shell: &Shell) -> ShellValue {
    let mut rng = rand::rng();
    let num: u32 = rng.random();
    let str = num.to_string();
    str.into()
}
