use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use volley_core::report::ReportExt as _;
use volley_core::transfer::Progress;

use crate::Generated;

const MANIFEST: &str = ".kestrel-codegen.json";

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    files: BTreeSet<String>,
}

pub(crate) fn validate_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || !path.components().all(|part| matches!(part, Component::Normal(_)))
        || path.extension().is_none_or(|extension| extension != "ts")
        || path.to_string_lossy().contains(['\\', ':'])
    {
        bail!("SDK files must use relative .ts paths without traversal: {}", path.display());
    }
    Ok(())
}

pub(crate) fn write_manifest(directory: &Path, generated: &Generated) -> Result<()> {
    let file = crate::writing::create_file(&directory.join(MANIFEST))?;
    serde_json::to_writer(&file, &manifest_data(generated))?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn manifest(generated: &Generated) -> Result<(&'static str, String)> {
    Ok((MANIFEST, serde_json::to_string(&manifest_data(generated))?))
}

fn manifest_data(generated: &Generated) -> Manifest {
    Manifest { version: 1, files: generated.files.keys().map(|path| portable_path(path)).collect() }
}

pub(crate) fn portable_path(path: &Path) -> String {
    path.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/")
}

pub(crate) fn check_directory(directory: &Path, overwrite: bool, progress: &Progress) -> Result<bool> {
    match std::fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            bail!("the SDK destination must be a directory, without a symlink")
        }
        Ok(_) => {}
    }
    if std::fs::read_dir(directory)?.next().transpose()?.is_none() {
        return Ok(true);
    }
    if !overwrite {
        bail!("{} already contains files; use --overwrite for an existing generated SDK", directory.display());
    }
    let marker = directory.join(MANIFEST);
    let metadata = std::fs::symlink_metadata(&marker).context("choose an empty directory or a Kestrel-generated SDK directory")?;
    if metadata.file_type().is_symlink() {
        bail!("the SDK manifest must not be a symlink");
    }
    let manifest: Manifest = serde_json::from_reader(std::fs::File::open(&marker)?)?;
    if manifest.version != 1 || manifest.files.is_empty() {
        bail!("unrecognized SDK manifest");
    }
    let files: BTreeSet<PathBuf> = manifest.files.into_iter().map(PathBuf::from).collect();
    let mut directories = BTreeSet::new();
    for file in &files {
        validate_path(file)?;
        for ancestor in file.ancestors().skip(1).filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(ancestor.to_path_buf());
        }
    }
    check_contents(directory, directory, &files, &directories, progress)?;
    Ok(true)
}

fn check_contents(
    root: &Path,
    directory: &Path,
    files: &BTreeSet<PathBuf>,
    directories: &BTreeSet<PathBuf>,
    progress: &Progress,
) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        progress.check()?;
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            bail!("SDK replacement refuses symlinks: {}", relative.display());
        }
        if metadata.is_dir() && directories.contains(relative) {
            check_contents(root, &path, files, directories, progress)?;
        } else if !metadata.is_file() || (relative != Path::new(MANIFEST) && !files.contains(relative)) {
            bail!("the SDK directory contains an unrelated file or directory: {}; choose a dedicated SDK destination", relative.display());
        }
    }
    Ok(())
}

pub(crate) fn file(temporary: &Path, destination: &Path, overwrite: bool) -> Result<()> {
    if overwrite {
        std::fs::rename(temporary, destination)?;
    } else {
        std::fs::hard_link(temporary, destination)?;
    }
    Ok(())
}

pub(crate) fn directory(temporary: &Path, destination: &Path, overwrite: bool, progress: &Progress) -> Result<()> {
    let exists = check_directory(destination, overwrite, progress)?;
    let backup = destination.parent().unwrap_or(Path::new(".")).join(format!(".kestrel-codegen-backup-{}", uuid::Uuid::new_v4()));
    if exists {
        std::fs::rename(destination, &backup)?;
    }
    if let Err(error) = std::fs::rename(temporary, destination) {
        if exists && let Err(restore) = std::fs::rename(&backup, destination) {
            return Err(error).context(format!(
                "SDK publication failed; restoring {} failed: {restore}; previous files remain in {}",
                destination.display(),
                backup.display()
            ));
        }
        return Err(error.into());
    }
    if exists {
        std::fs::remove_dir_all(&backup).report_err("remove previous generated SDK");
    }
    Ok(())
}
