use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use kestrel_codegen::DataLayer;

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum QueryFlavor {
    React,
    Vue,
}

impl From<QueryFlavor> for DataLayer {
    fn from(flavor: QueryFlavor) -> Self {
        match flavor {
            QueryFlavor::React => Self::TanstackQueryReact,
            QueryFlavor::Vue => Self::TanstackQueryVue,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "kestrelcli",
    version,
    about = "Kestrel from the terminal: run requests, import, export, generate SDKs and sync collections"
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(super) enum Command {
    #[command(about = "Serve a collection over MCP on stdin/stdout")]
    Mcp {
        #[arg(default_value = ".")]
        collection: PathBuf,
    },
    Codegen {
        #[arg(default_value = ".")]
        collection: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value = "javascript", value_parser = ["javascript"])]
        language: String,
        #[arg(long)]
        single_file: bool,
        #[arg(long)]
        archive: bool,
        #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "react", conflicts_with = "rstore")]
        tanstack_query: Option<QueryFlavor>,
        #[arg(long)]
        rstore: bool,
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
