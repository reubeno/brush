//! Analysis commands for benchmarks and API diffing.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Run analysis and comparison tools.
#[derive(Parser)]
pub enum AnalyzeCommand {
    /// Run benchmarks and output results.
    Bench(BenchArgs),
    /// Compare public API against a base branch.
    PublicApi(PublicApiArgs),
}

/// Arguments for benchmark analysis.
#[derive(Parser)]
pub struct BenchArgs {
    /// Output file for benchmark results (bencher format).
    #[clap(long, short = 'o')]
    output: Option<PathBuf>,
}

/// Arguments for public API analysis.
#[derive(Parser)]
pub struct PublicApiArgs {
    /// Base branch to compare against (e.g., 'main' or 'origin/main').
    #[clap(long, short = 'b', default_value = "origin/main")]
    base: String,

    /// Output directory for API diff reports.
    #[clap(long, short = 'o', default_value = "reports")]
    output_dir: PathBuf,
}

/// Run an analysis command.
pub fn run(cmd: &AnalyzeCommand) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        AnalyzeCommand::Bench(args) => run_bench(&sh, args),
        AnalyzeCommand::PublicApi(args) => run_public_api(&sh, args),
    }
}

fn run_bench(sh: &Shell, args: &BenchArgs) -> Result<()> {
    eprintln!("Running benchmarks...");

    if let Some(output) = &args.output {
        // Run with output capture to file using tee
        let bench_output = cmd!(
            sh,
            "cargo bench --workspace --benches -- --output-format bencher"
        )
        .read()
        .context("Benchmarks failed")?;

        // Write to file
        sh.write_file(output, &bench_output)?;
        // Also print to stdout
        println!("{bench_output}");
    } else {
        // Run without file output
        cmd!(sh, "cargo bench --workspace --benches")
            .run()
            .context("Benchmarks failed")?;
    }

    eprintln!("Benchmarks completed.");
    Ok(())
}

fn run_public_api(sh: &Shell, args: &PublicApiArgs) -> Result<()> {
    eprintln!("Analyzing public API against {}...", args.base);

    // Ensure cargo-public-api is available
    cmd!(sh, "cargo +nightly public-api --version")
        .run()
        .context("cargo-public-api not installed. Install with: cargo install cargo-public-api")?;

    // Create output directories
    let diffs_dir = args.output_dir.join("diffs");
    let reports_dir = args.output_dir.clone();

    sh.create_dir(&diffs_dir)?;
    sh.create_dir(&reports_dir)?;

    // Get list of library crates
    let crates_output = cmd!(sh, "./scripts/enum-lib-crates.sh")
        .read()
        .context("Failed to enumerate library crates")?;

    let crates: Vec<&str> = crates_output.lines().collect();

    if crates.is_empty() {
        eprintln!("No library crates found.");
        return Ok(());
    }

    let base = &args.base;

    for crate_name in crates {
        eprintln!("Analyzing crate: {crate_name}");

        let diff_file = diffs_dir.join(format!("{crate_name}.txt"));
        let report_file = reports_dir.join(format!("api-diff-{crate_name}.md"));

        let diff_path = diff_file.display().to_string();
        let report_path = report_file.display().to_string();

        // Run public-api diff
        // Use unchecked because diff returns non-zero if there are differences
        let diff_result = cmd!(
            sh,
            "cargo +nightly public-api diff -sss -p {crate_name} {base}..HEAD"
        )
        .ignore_status()
        .read();

        match diff_result {
            Ok(diff_output) => {
                sh.write_file(&diff_file, &diff_output)?;

                // Format the report using the Python script
                let format_result = cmd!(
                    sh,
                    "./scripts/format-api-diff-report.py {diff_path} -p {crate_name}"
                )
                .read();

                match format_result {
                    Ok(report) => {
                        if report.trim().is_empty() {
                            eprintln!("  No API changes for {crate_name}");
                        } else {
                            sh.write_file(&report_file, &report)?;
                            eprintln!("  Report written to {report_path}");
                        }
                    }
                    Err(e) => {
                        eprintln!("  Warning: Failed to format report for {crate_name}: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("  Warning: Failed to analyze {crate_name}: {e}");
            }
        }
    }

    eprintln!(
        "Public API analysis complete. Reports in: {}",
        reports_dir.display()
    );
    Ok(())
}
