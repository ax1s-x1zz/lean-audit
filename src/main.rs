//! `lean-audit` — read-only auditor for Lean 4 research repositories.

mod graph;
mod hash;
mod inspect;
mod lean_src;
mod manifest;
mod model;
mod report;
mod util;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use model::{DISCLAIMER, SCHEMA};

#[derive(Parser)]
#[command(
    name = "lean-audit",
    version,
    about = "Read-only auditor for Lean 4 research repositories",
    long_about = "Re-computes the verification facts a Lean 4 repository publishes about itself \
                  (toolchain pins, manifest hashes, head-theorem axioms, direct `sorry` usage) \
                  and reports whether they still hold in the checkout you have.\n\n\
                  It does NOT check mathematics and does NOT replace `Audit.lean`."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Toolchain pins, lakefile, lake-manifest, modules, import graph, audit detection.
    Inspect {
        /// Path to the repository root.
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Md)]
        format: Format,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the module import graph as JSON to this path.
        #[arg(long)]
        emit_graph: Option<PathBuf>,
        /// Skip SHA-256 re-computation of the repository's manifests.
        #[arg(long)]
        no_verify: bool,
    },
    /// Verify the repository's manifests by re-computing their SHA-256 hashes.
    Manifest {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Md)]
        format: Format,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Comment- and string-aware scan for forbidden tokens (sorry, axiom, unsafe, ...).
    Scan {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Md)]
        format: Format,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Inspect every Lean project in a directory and summarise them together.
    Batch {
        /// Directory whose immediate subdirectories are Lean projects.
        dir: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Md)]
        format: Format,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Skip SHA-256 re-computation of the repositories' manifests.
        #[arg(long)]
        no_verify: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Md,
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect {
            path,
            format,
            out,
            emit_graph,
            no_verify,
        } => {
            let opts = inspect::InspectOptions {
                emit_graph,
                verify_hashes: !no_verify,
            };
            let report = inspect::inspect(&path, &opts)?;
            let rendered = match format {
                Format::Json => report::to_json(&report)?,
                Format::Md => report::inspect_markdown(&report),
            };
            emit(&rendered, out.as_deref())?;

            // Non-zero exit when the repository contains forbidden tokens, so the
            // tool can gate CI. Absence of tokens is not a correctness claim.
            if !report.scan.flags.is_empty() {
                std::process::exit(2);
            }
        }
        Command::Manifest { path, format, out } => {
            let report = inspect::inspect(
                &path,
                &inspect::InspectOptions {
                    emit_graph: None,
                    verify_hashes: true,
                },
            )?;
            let manifest_report = report::ManifestReport {
                schema: format!("{SCHEMA}/manifest"),
                generated_at: model::now_iso8601(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
                project: report.project.clone(),
                manifests: report.manifests.clone(),
                disclaimer: DISCLAIMER.to_string(),
            };
            let rendered = match format {
                Format::Json => report::to_json(&manifest_report)?,
                Format::Md => report::manifest_markdown(&manifest_report),
            };
            emit(&rendered, out.as_deref())?;

            if manifest_report
                .manifests
                .iter()
                .any(|m| m.status == "mismatch")
            {
                std::process::exit(3);
            }
        }
        Command::Scan { path, format, out } => {
            let report = inspect::inspect(
                &path,
                &inspect::InspectOptions {
                    emit_graph: None,
                    verify_hashes: false,
                },
            )?;
            let scan_report = report::ScanReport {
                schema: format!("{SCHEMA}/scan"),
                generated_at: model::now_iso8601(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
                project: report.project.clone(),
                scan: report.scan.clone(),
                disclaimer: DISCLAIMER.to_string(),
            };
            let rendered = match format {
                Format::Json => report::to_json(&scan_report)?,
                Format::Md => report::scan_markdown(&scan_report),
            };
            emit(&rendered, out.as_deref())?;
            if !scan_report.scan.flags.is_empty() {
                std::process::exit(2);
            }
        }
        Command::Batch {
            dir,
            format,
            out,
            no_verify,
        } => {
            let reports = inspect::batch(&dir, !no_verify)?;
            if reports.is_empty() {
                anyhow::bail!("no Lean projects found under {}", dir.display());
            }
            let batch = model::BatchReport {
                schema: format!("{SCHEMA}/batch"),
                generated_at: model::now_iso8601(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
                root: dir.to_string_lossy().to_string(),
                repos: reports,
                disclaimer: DISCLAIMER.to_string(),
            };
            let rendered = match format {
                Format::Json => report::to_json(&batch)?,
                Format::Md => report::batch_markdown(&batch),
            };
            emit(&rendered, out.as_deref())?;

            if batch.repos.iter().any(|r| !r.scan.flags.is_empty()) {
                std::process::exit(2);
            }
        }
    }
    Ok(())
}

fn emit(text: &str, out: Option<&std::path::Path>) -> Result<()> {
    match out {
        Some(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        }
        None => println!("{text}"),
    }
    Ok(())
}
