use std::io::Write as _;
use std::path::Path;

use anyhow::Result;
use volley_core::transfer::Progress;
use zip::write::SimpleFileOptions;

pub(crate) fn write(path: &Path, generated: &crate::Generated, single_file: bool, progress: &Progress) -> Result<()> {
    let mut archive = zip::ZipWriter::new(crate::writing::create_file(path)?);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)?)
        .unix_permissions(0o644);
    for (path, source) in &generated.files {
        progress.check()?;
        let name = crate::publishing::portable_path(path);
        archive.start_file(name, options)?;
        crate::writing::write_chunks(&mut archive, source, progress)?;
    }
    if !single_file {
        progress.check()?;
        let (name, source) = crate::publishing::manifest(generated)?;
        archive.start_file(name, options)?;
        archive.write_all(source.as_bytes())?;
    }
    archive.finish()?.sync_all()?;
    Ok(())
}
