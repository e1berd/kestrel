use std::path::Path;

use anyhow::Result;

use super::{YELLOW, paint};

pub(super) fn generate(collection: &Path, destination: &Path, overwrite: bool) -> Result<()> {
    let monitor = super::transfer::Monitor::new();
    let generated = volley_codegen::generate_file(collection, destination, overwrite, &monitor.progress)?;
    drop(monitor);
    for warning in &generated.warnings {
        eprintln!("{} {warning}", paint("warning:", YELLOW));
    }
    println!("Generated TypeScript SDK: {} methods in {}", generated.requests, destination.display());
    for method in &generated.methods {
        println!("  {method}");
    }
    Ok(())
}
