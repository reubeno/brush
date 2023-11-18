use std::{collections::HashMap, path::PathBuf};

pub struct ExecutionContext {
    // TODO: open files
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub parameters: HashMap<String, String>,
    pub funcs: HashMap<String, ShellFunction>,
    pub options: ShellOptions,
    // TODO: async lists
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    //
    pub last_pipeline_exit_status: u32,
}

pub struct ShellOptions {
    // TODO: Add other options.
}

impl Default for ShellOptions {
    fn default() -> Self {
        Self {}
    }
}

type ShellFunction = parser::ast::FunctionDefinition;
