#[derive(clap::Parser)]
struct Command {
    args: Vec<String>,
}

brush_builtin_clap::impl_parse_args!(Command, keep_double_dash = "args");

fn main() {}
