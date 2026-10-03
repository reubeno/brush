#[derive(clap::Parser)]
struct Command {
    args: Vec<String>,
}

brush_builtin_utils::clap_builtin!(Command, trailing_args = "args");

fn main() {}
