use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::{Result, bail};
use kestrel_core::Collection;
use kestrel_core::report::ReportExt as _;
use kestrel_core::transfer::{Progress, Unit};

use crate::{Generated, Generator, OutputOptions, archive, bundled_output, publishing};

pub fn generate_to(source: &Path, destination: &Path, options: OutputOptions, overwrite: bool, progress: &Progress) -> Result<Generated> {
    Generator::new().output(options).generate_to(source, destination, overwrite, progress)
}

pub(crate) fn generate_with(
    generator: &Generator,
    source: &Path,
    destination: &Path,
    overwrite: bool,
    progress: &Progress,
) -> Result<Generated> {
    let options = generator.output;
    validate_destination(destination, options, overwrite, progress)?;
    progress.start("Reading saved collection", None, Unit::Files)?;
    let collection = Collection::open_with_progress(source, progress)?;
    let generated = generator.generate(&collection, progress)?;
    for path in generated.files.keys() {
        publishing::validate_path(path)?;
    }
    let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".kestrel-codegen-{}", uuid::Uuid::new_v4()));
    let directory = !options.archive && !options.single_file;
    let result = (|| {
        let bytes = generated.files.values().map(|source| source.len() as u64).sum();
        progress.start(if options.archive { "Packing SDK as ZIP" } else { "Writing SDK files" }, Some(bytes), Unit::Bytes)?;
        if options.archive {
            archive::write(&temporary, &generated, options.single_file, progress)?;
        } else if options.single_file {
            bundled_output::write(&temporary, destination, &generated, progress)?;
        } else {
            std::fs::create_dir(&temporary)?;
            for (path, source) in &generated.files {
                write_source(&temporary.join(path), source, progress)?;
            }
            publishing::write_manifest(&temporary, &generated)?;
        }
        progress.check()?;
        if directory {
            publishing::directory(&temporary, destination, overwrite, progress)?;
        } else if options.single_file && !options.archive {
            bundled_output::publish(&temporary, destination, overwrite, progress)?;
        } else {
            publishing::file(&temporary, destination, overwrite)?;
        }
        progress.published("SDK generated", generated.files.len() as u64, Unit::Files);
        Ok(generated)
    })();
    let retained_backup = result.is_err() && (temporary.join("previous-0").exists() || temporary.join("previous-1").exists());
    if temporary.exists() && !retained_backup {
        if temporary.is_dir() {
            std::fs::remove_dir_all(&temporary).report_err("remove SDK staging directory");
        } else {
            std::fs::remove_file(&temporary).report_err("remove SDK temporary file");
        }
    }
    result
}

fn validate_destination(destination: &Path, options: OutputOptions, overwrite: bool, progress: &Progress) -> Result<()> {
    progress.check()?;
    if !options.single_file && !options.archive {
        if destination.extension().is_some_and(|extension| extension == "js" || extension == "ts" || extension == "zip") {
            bail!("the default SDK output is a directory; use --single-file for .js or --archive for .zip");
        }
        publishing::check_directory(destination, overwrite, progress)?;
    } else if !options.archive {
        bundled_output::validate(destination, overwrite)?;
    } else {
        let extension = "zip";
        if destination.extension().is_none_or(|actual| actual != extension) {
            bail!("the SDK destination must end in .{extension}");
        }
        match std::fs::symlink_metadata(destination) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                bail!("the SDK file destination must be a regular file")
            }
            Ok(_) if !overwrite => bail!("{} already exists; use --overwrite to replace it", destination.display()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(crate) fn write_source(path: &Path, source: &str, progress: &Progress) -> Result<()> {
    std::fs::create_dir_all(path.parent().expect("a generated file has a parent"))?;
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = progress.writer(file);
    write_chunks(&mut writer, source, progress)?;
    writer.get_ref().sync_all()?;
    Ok(())
}

pub(crate) fn write_chunks(writer: &mut impl std::io::Write, source: &str, progress: &Progress) -> Result<()> {
    for chunk in source.as_bytes().chunks(64 * 1024) {
        progress.check()?;
        writer.write_all(chunk)?;
        progress.advance(chunk.len() as u64)?;
    }
    Ok(())
}

pub(crate) fn create_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new().read(true).write(true).create_new(true).open(path)?)
}
