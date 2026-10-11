#[derive(clap::Parser)]
struct Command {
    args: Vec<String>,
}

brush_builtin_utils::clap_builtin!(Command, trailing = args);

fn main() {}
