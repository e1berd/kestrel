use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "kestrel-cli",
    version,
    about = "Kestrel from the terminal: run requests, import, export, generate SDKs and sync collections"
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(super) enum Command {
    Codegen {
        #[arg(default_value = ".")]
        collection: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value = "typescript", value_parser = ["typescript"])]
        language: String,
        #[arg(long)]
        overwrite: bool,
    },
    Export {
        #[arg(default_value = ".")]
        collection: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value = "openapi32", value_parser = ["openapi30", "openapi31", "openapi32", "swagger20", "apidog"])]
        format: String,
        #[arg(long, default_value = "json", value_parser = ["json", "yaml"])]
        encoding: String,
        #[arg(long)]
        overwrite: bool,
    },
    Run {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        env: Option<String>,
        #[arg(long = "var", value_parser = parse_pair)]
        vars: Vec<(String, String)>,
        #[arg(long)]
        bail: bool,
        #[arg(short, long)]
        verbose: bool,
        #[arg(long)]
        json: bool,
    },
    Import {
        spec: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long, default_value = "tags")]
        group_by: String,
        #[arg(long, default_value = "auto", value_parser = ["auto", "openapi", "apidog"])]
        format: String,
    },
    Sync {
        #[arg(default_value = ".")]
        collection: PathBuf,
        #[arg(long)]
        spec: Option<String>,
        #[arg(long)]
        group_by: Option<String>,
        #[arg(long)]
        prune: bool,
        #[arg(long)]
        dry_run: bool,
    },
    Envs {
        #[arg(default_value = ".")]
        collection: PathBuf,
    },
}

fn parse_pair(text: &str) -> Result<(String, String), String> {
    text.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).ok_or_else(|| format!("`{text}` is not name=value"))
}
