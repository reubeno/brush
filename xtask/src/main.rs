use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser};

#[derive(Parser)]
struct CommandLineArgs {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser)]
enum Command {
    /// Generate man content.
    GenerateMan(GenerateManArgs),
    /// Generate help content in markdown format.
    GenerateMarkdown(GenerateMarkdownArgs),
}

#[derive(Parser)]
struct GenerateManArgs {
    /// Output directory.
    #[clap(long = "output-dir", short = 'o')]
    output_dir: PathBuf,
}

#[derive(Parser)]
struct GenerateMarkdownArgs {
    /// Output directory.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,
}

fn main() -> Result<()> {
    let args = CommandLineArgs::parse();

    match &args.command {
        Command::GenerateMan(gen_args) => generate_man(gen_args),
        Command::GenerateMarkdown(gen_args) => generate_markdown(gen_args),
    }
}

fn generate_man(args: &GenerateManArgs) -> Result<()> {
    // Create the output dir if it doesn't exist. If it already does, we proceed
    // onward and hope for the best.
    if !args.output_dir.exists() {
        std::fs::create_dir_all(&args.output_dir)?;
    }

    // Generate!
    let cmd = brush_shell::args::CommandLineArgs::command();
    clap_mangen::generate_to(cmd, &args.output_dir)?;

    Ok(())
}

fn generate_markdown(args: &GenerateMarkdownArgs) -> Result<()> {
    let options = clap_markdown::MarkdownOptions::new()
        .show_footer(false)
        .show_table_of_contents(true);

    // Generate!
    let markdown =
        clap_markdown::help_markdown_custom::<brush_shell::args::CommandLineArgs>(&options);
    std::fs::write(&args.output_path, markdown)?;

    Ok(())
}
