//! Generates artifacts derived from the `brush` command-line interface: man
//! pages, markdown help, shell completion scripts, and the config file's JSON
//! schema.
//!
//! Man pages and markdown are rendered from the command line's help data.
//! Completion scripts call back into `brush` itself for their answers, so they
//! need no other tool installed.
//!
//! This lives here, rather than in `xtask`, so that the build tooling doesn't
//! need to depend on the shell it builds. It's driven by `cargo xtask gen`.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result};
use winnow_args::Args;
use winnow_args::complete::Shell;
use winnow_args::help::{Command, Item};

/// Generate artifacts derived from the brush command-line interface.
#[derive(Args)]
#[arg(name = "gen")]
struct GenCli {
    #[arg(subcommand)]
    command: GenCommand,
}

#[derive(winnow_args::Subcommand)]
enum GenCommand {
    /// Generate man content into the given directory.
    Man(OutputDir),
    /// Generate help content in markdown format.
    Markdown(OutputPath),
    /// Generate a completion script, written to standard output.
    Completion(ShellName),
    /// Generate the JSON schema for the configuration file.
    Schema(OutputPath),
}

#[derive(Args)]
struct OutputDir {
    /// Output directory.
    #[arg(positional)]
    output_dir: PathBuf,
}

#[derive(Args)]
struct OutputPath {
    /// Output file path.
    #[arg(positional)]
    output_path: PathBuf,
}

#[derive(Args)]
struct ShellName {
    /// Shell to generate a completion script for: bash, fish, or zsh.
    #[arg(positional)]
    shell: String,
}

fn main() -> Result<()> {
    let GenCli { command } = GenCli::parse();
    let cli = brush_shell::args::CommandLineArgs::HELP;

    match command {
        GenCommand::Man(OutputDir { output_dir }) => {
            std::fs::create_dir_all(&output_dir)?;
            std::fs::write(output_dir.join("brush.1"), manpage(cli))?;
        }
        GenCommand::Markdown(OutputPath { output_path }) => {
            std::fs::write(output_path, format!("{}\n", markdown(cli).trim()))?;
        }
        GenCommand::Completion(ShellName { shell }) => {
            let shell = Shell::from_name(&shell)
                .with_context(|| format!("no completion script for shell `{shell}`"))?;
            print!(
                "{}",
                brush_shell::args::CommandLineArgs::completion_script(shell)
            );
        }
        GenCommand::Schema(OutputPath { output_path }) => {
            let schema = schemars::schema_for!(brush_shell::config::Config);
            let json = serde_json::to_string_pretty(&schema)?;
            std::fs::write(output_path, format!("{json}\n"))?;
        }
    }

    Ok(())
}

/// A flag's spelling as help shows it: `-l, --login`, `--config <FILE>`.
fn spelling(item: &Item) -> String {
    let mut out = String::new();
    if let Some(short) = item.short {
        let _ = write!(out, "-{short}");
    }
    if let Some(long) = item.long {
        if !out.is_empty() {
            out.push_str(", ");
        }
        let _ = write!(out, "--{long}");
    }
    if let Some(value) = item.value_name {
        let _ = write!(out, " <{value}>");
    }
    out
}

/// The visible flags, grouped by heading in order of first appearance.
fn sections(cli: &Command) -> Vec<(&'static str, Vec<&Item>)> {
    let mut sections: Vec<(&'static str, Vec<&Item>)> = Vec::new();
    for item in cli.items.iter().filter(|i| !i.positional && !i.hide) {
        let heading = item.heading.unwrap_or("Options");
        match sections.iter_mut().find(|(h, _)| *h == heading) {
            Some((_, items)) => items.push(item),
            None => sections.push((heading, vec![item])),
        }
    }
    sections
}

fn arguments(cli: &Command) -> impl Iterator<Item = &Item> {
    cli.items.iter().filter(|i| i.positional && !i.hide)
}

/// `brush.1`, in roff.
fn manpage(cli: &Command) -> String {
    let roff = |text: &str| text.replace('\\', "\\\\").replace('-', "\\-");
    let mut out = String::new();
    let _ = writeln!(out, ".TH {} 1", cli.name.to_uppercase());
    let _ = writeln!(out, ".SH NAME\n{} \\- {}", cli.name, roff(cli.about));
    let _ = writeln!(
        out,
        ".SH SYNOPSIS\n\\fB{}\\fR [\\fIOPTIONS\\fR] [\\fISCRIPT_PATH\\fR [\\fISCRIPT_ARGS\\fR]...]",
        cli.name
    );
    let _ = writeln!(out, ".SH DESCRIPTION");
    for paragraph in cli.long_about.split("\n\n") {
        let _ = writeln!(out, "{}\n.PP", roff(paragraph));
    }
    for (heading, items) in sections(cli) {
        let _ = writeln!(out, ".SH {}", roff(&heading.to_uppercase()));
        for item in items {
            let _ = writeln!(
                out,
                ".TP\n\\fB{}\\fR\n{}",
                roff(&spelling(item)),
                roff(item.long_help)
            );
        }
    }
    for item in arguments(cli) {
        let name = item.value_name.unwrap_or("ARG");
        let _ = writeln!(
            out,
            ".SH ARGUMENTS\n.TP\n\\fI{}\\fR\n{}",
            roff(name),
            roff(item.long_help)
        );
    }
    out
}

/// Help in markdown, for the documentation site.
fn markdown(cli: &Command) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# `{}`\n\n{}\n", cli.name, cli.long_about);
    let _ = writeln!(
        out,
        "**Usage:** `{} [OPTIONS] [SCRIPT_PATH [SCRIPT_ARGS]...]`\n",
        cli.name
    );
    for item in arguments(cli) {
        let name = item.value_name.unwrap_or("ARG");
        let _ = writeln!(out, "## Arguments\n\n* `{name}` — {}\n", item.help);
    }
    for (heading, items) in sections(cli) {
        let _ = writeln!(out, "## {heading}\n");
        for item in items {
            let _ = write!(out, "* `{}` — {}", spelling(item), item.help);
            if !item.choices.is_empty() {
                let _ = write!(
                    out,
                    "\n\n  Possible values: `{}`",
                    item.choices.join("`, `")
                );
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}
