use std::{collections::HashMap, path::PathBuf};

pub struct ExecutionContext {
    // TODO: open files
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub parameters: HashMap<String, String>,
    pub funcs: HashMap<String, ShellFunction>,
    // TODO: options
    // TODO: async lists
    pub aliases: HashMap<String, String>,
}

type ShellFunction = parser::ast::FunctionDefinition;
