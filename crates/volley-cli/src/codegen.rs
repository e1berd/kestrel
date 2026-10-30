use std::path::Path;

use anyhow::Result;

use super::{YELLOW, paint};

pub(super) fn generate(collection: &Path, destination: &Path, options: volley_codegen::OutputOptions, overwrite: bool) -> Result<()> {
    let monitor = super::transfer::Monitor::new();
    let generated = volley_codegen::generate_to(collection, destination, options, overwrite, &monitor.progress)?;
    drop(monitor);
    for warning in &generated.warnings {
        eprintln!("{} {warning}", paint("warning:", YELLOW));
    }
    println!(
        "Generated TypeScript SDK: {} methods, {} source files in {}",
        generated.requests,
        generated.files.len(),
        destination.display()
    );
    for method in &generated.methods {
        println!("  {method}");
    }
    Ok(())
}
