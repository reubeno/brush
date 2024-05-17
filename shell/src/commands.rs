use std::fmt::Display;

use parser::ast;

#[derive(Clone, Debug)]
pub enum CommandArg {
    String(String),
    Assignment(ast::Assignment),
}

impl Display for CommandArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandArg::String(s) => f.write_str(s),
            CommandArg::Assignment(a) => write!(f, "{a}"),
        }
    }
}

impl From<String> for CommandArg {
    fn from(s: String) -> Self {
        CommandArg::String(s)
    }
}

impl From<&String> for CommandArg {
    fn from(value: &String) -> Self {
        CommandArg::String(value.clone())
    }
}
