use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser)]
#[command(name = "kestrel-mcp", version, about = "Serve a Kestrel collection over MCP on stdin/stdout")]
struct Arguments {
    #[arg(default_value = ".")]
    collection: PathBuf,
}

fn main() -> ExitCode {
    match kestrel_mcp::serve(&Arguments::parse().collection) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            kestrel_core::report::report_error("serve the MCP collection", error);
            ExitCode::FAILURE
        }
    }
}
