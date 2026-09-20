struct Command {
    args: Vec<brush_core::CommandArg>,
}

brush_builtin_utils::verbatim_builtin!(
    Command,
    raw_args = args,
    synopsis = "command",
    description = "a command",
);

fn main() {}
