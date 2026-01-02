//! Generation commands for documentation, completions, and schemas.

use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser};

/// Generate various artifacts.
#[derive(Parser)]
pub enum GenCommand {
    /// Generate completion scripts.
    #[clap(subcommand)]
    Completion(CompletionCommand),
    /// Generate documentation.
    #[clap(subcommand)]
    Docs(DocsCommand),
    /// Generate JSON schemas.
    #[clap(subcommand)]
    Schema(SchemaCommand),
}

/// Documentation generation commands.
#[derive(Parser)]
pub enum DocsCommand {
    /// Generate man content.
    Man(GenerateManArgs),
    /// Generate help content in markdown format.
    Markdown(GenerateMarkdownArgs),
}

/// Completion script generation commands.
#[derive(Parser)]
pub enum CompletionCommand {
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

/// Arguments for man page generation.
#[derive(Parser)]
pub struct GenerateManArgs {
    /// Output directory.
    #[clap(long = "output-dir", short = 'o')]
    output_dir: PathBuf,
}

/// Arguments for markdown documentation generation.
#[derive(Parser)]
pub struct GenerateMarkdownArgs {
    /// Output directory.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,
}

/// Schema generation commands.
#[derive(Parser)]
pub enum SchemaCommand {
    /// Generate JSON schema for the configuration file.
    Config(GenerateSchemaArgs),
}

/// Arguments for schema generation.
#[derive(Parser)]
pub struct GenerateSchemaArgs {
    /// Output file path.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,
}

/// Run a generation command.
pub fn run(cmd: &GenCommand) -> Result<()> {
    match cmd {
        GenCommand::Docs(docs_cmd) => match docs_cmd {
            DocsCommand::Man(args) => gen_man(args),
            DocsCommand::Markdown(args) => gen_markdown_docs(args),
        },
        GenCommand::Completion(completion_cmd) => {
            match completion_cmd {
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
        GenCommand::Schema(schema_cmd) => match schema_cmd {
            SchemaCommand::Config(args) => gen_config_schema(args),
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
    std::fs::write(&args.output_path, format!("{json}\n"))?;

    Ok(())
}
