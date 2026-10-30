use std::path::Path;

use anyhow::Result;
use kestrel_import::{Format, PreparedImport};
use kestrel_openapi::GroupBy;

use super::{GREEN, YELLOW, paint};

pub(super) fn import(source: &str, out: &Path, name: Option<&str>, group_by: &str, format: &str) -> Result<()> {
    let format = if format == "auto" { None } else { Some(Format::parse(format)?) };
    let monitor = super::transfer::Monitor::new();
    let prepared =
        PreparedImport::load_with_progress(source, &std::env::current_dir()?, format, GroupBy::parse(group_by), &monitor.progress)?;
    for warning in &prepared.preview.warnings {
        eprintln!("{} {warning}", paint("warning:", YELLOW));
    }
    let root = prepared.save_with_progress(out, name, &monitor.progress)?;
    drop(monitor);
    println!(
        "{} {} in {}: {} requests, {} folders",
        paint("Imported", GREEN),
        name.unwrap_or(&prepared.preview.title),
        root.display(),
        prepared.preview.requests,
        prepared.preview.folders
    );
    Ok(())
}

pub(super) fn export(source: &Path, out: &Path, format: &str, encoding: &str, overwrite: bool) -> Result<()> {
    use kestrel_import::export::{Encoding, ExportFormat};
    let monitor = super::transfer::Monitor::new();
    let format = ExportFormat::parse(format)?;
    let encoding = if encoding == "yaml" { Encoding::Yaml } else { Encoding::Json };
    let report = kestrel_import::export::export(source, out, format, encoding, overwrite, &monitor.progress)?;
    drop(monitor);
    for warning in report.warnings {
        eprintln!("{} {warning}", paint("warning:", YELLOW));
    }
    println!("Exported {} requests to {} ({} bytes)", report.requests, out.display(), report.bytes);
    Ok(())
}
