#[derive(clap::Parser)]
struct Command {
    args: Vec<String>,
    #[clap(skip)]
    declarations: Vec<brush_core::CommandArg>,
}

brush_builtin_utils::clap_builtin!(Command, trailing_args = args, declarations = declarations);

fn main() {}
