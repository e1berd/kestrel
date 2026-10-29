use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use volley_core::Collection;
use volley_core::report::ReportExt as _;
use volley_core::transfer::{Progress, Unit};

use crate::Generated;

pub fn generate_file(source: &Path, destination: &Path, overwrite: bool, progress: &Progress) -> Result<Generated> {
    if destination.extension().is_none_or(|extension| extension != "ts") {
        bail!("the TypeScript SDK destination must end in .ts");
    }
    if destination.exists() && !overwrite {
        bail!("{} already exists; use --overwrite to replace it", destination.display());
    }
    progress.start("Reading saved collection", None, Unit::Files)?;
    let collection = Collection::open_with_progress(source, progress)?;
    let generated = crate::generate(&collection, progress)?;
    let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".kestrel-codegen-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        progress.start("Writing TypeScript SDK", Some(generated.source.len() as u64), Unit::Bytes)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).context("create SDK temporary file")?;
        file.write_all(generated.source.as_bytes())?;
        file.sync_all()?;
        drop(file);
        progress.advance(generated.source.len() as u64)?;
        progress.check()?;
        if overwrite {
            std::fs::rename(&temporary, destination)?;
        } else {
            std::fs::hard_link(&temporary, destination)?;
        }
        progress.published("SDK generated", generated.requests as u64, Unit::Requests);
        Ok(generated)
    })();
    if temporary.exists() {
        std::fs::remove_file(&temporary).report_err("remove SDK temporary file");
    }
    result
}
