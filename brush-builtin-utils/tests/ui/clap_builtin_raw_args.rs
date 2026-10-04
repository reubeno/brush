#[derive(clap::Parser)]
struct Command {
    #[clap(skip)]
    args: Vec<brush_core::CommandArg>,
}

brush_builtin_utils::clap_builtin!(Command, raw_args = args);

fn main() {}
