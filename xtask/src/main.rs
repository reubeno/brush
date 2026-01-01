//! xtask-style command-line tool for building this project.

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
    /// Generate documentation.
    #[clap(subcommand)]
    Docs(DocsCommand),
    /// Generate completion scripts.
    #[clap(subcommand)]
    Completion(CompletionCommand),
    /// Generate JSON schemas.
    #[clap(subcommand)]
    Schema(SchemaCommand),
}

#[derive(Parser)]
enum DocsCommand {
    /// Generate man content.
    Man(GenerateManArgs),
    /// Generate help content in markdown format.
    Markdown(GenerateMarkdownArgs),
}

#[derive(Parser)]
enum CompletionCommand {
    /// Generate completion script for `bash`.
    Bash,
    /// Generate completion script for `elvish`.
    Elvish,
    /// Generate completion script for `fish`.
    Fish,
    /// Generate completion script for `PowerShell`.
    PowerShell,
    /// Generate completion script for `zsh`.
    Zsh,
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

#[derive(Parser)]
enum SchemaCommand {
    /// Generate JSON schema for the configuration file.
    Config(GenerateSchemaArgs),
}

#[derive(Parser)]
struct GenerateSchemaArgs {
    /// Output file path.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,
}

fn main() -> Result<()> {
    let args = CommandLineArgs::parse();

    match &args.command {
        Command::Docs(cmd) => match cmd {
            DocsCommand::Man(gen_args) => gen_man(gen_args),
            DocsCommand::Markdown(gen_args) => gen_markdown_docs(gen_args),
        },
        Command::Completion(cmd) => {
            match cmd {
                CompletionCommand::Bash => gen_completion_script(clap_complete::Shell::Bash),
                CompletionCommand::Elvish => gen_completion_script(clap_complete::Shell::Elvish),
                CompletionCommand::Fish => gen_completion_script(clap_complete::Shell::Fish),
                CompletionCommand::PowerShell => {
                    gen_completion_script(clap_complete::Shell::PowerShell);
                }
                CompletionCommand::Zsh => gen_completion_script(clap_complete::Shell::Zsh),
            }

            Ok(())
        }
        Command::Schema(cmd) => match cmd {
            SchemaCommand::Config(gen_args) => gen_config_schema(gen_args),
        },
    }
}

fn gen_man(args: &GenerateManArgs) -> Result<()> {
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

fn gen_markdown_docs(args: &GenerateMarkdownArgs) -> Result<()> {
    let options = clap_markdown::MarkdownOptions::new()
        .show_footer(false)
        .show_table_of_contents(true);

    // Generate!
    let markdown =
        clap_markdown::help_markdown_custom::<brush_shell::args::CommandLineArgs>(&options);
    std::fs::write(&args.output_path, markdown)?;

    Ok(())
}

fn gen_completion_script(shell: clap_complete::Shell) {
    let mut cmd = brush_shell::args::CommandLineArgs::command();
    clap_complete::generate(shell, &mut cmd, "brush", &mut std::io::stdout());
}

fn gen_config_schema(args: &GenerateSchemaArgs) -> Result<()> {
    // Generate JSON schema for the configuration file.
    let schema = schemars::schema_for!(brush_shell::config::Config);
    let json = serde_json::to_string_pretty(&schema)?;
    std::fs::write(&args.output_path, json)?;

    Ok(())
}
