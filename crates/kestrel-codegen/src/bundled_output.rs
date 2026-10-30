use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use kestrel_core::report::ReportExt as _;
use kestrel_core::transfer::Progress;

use crate::{Generated, publishing};

fn destinations(destination: &Path) -> [PathBuf; 2] {
    [destination.to_path_buf(), destination.with_extension("d.ts")]
}

pub(crate) fn validate(destination: &Path, overwrite: bool) -> Result<()> {
    if destination.extension().is_none_or(|extension| extension != "js") {
        bail!("the SDK destination must end in .js");
    }
    for path in destinations(destination) {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                bail!("the SDK file destination must be a regular file: {}", path.display())
            }
            Ok(_) if !overwrite => bail!("{} already exists; use --overwrite to replace it", path.display()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(crate) fn write(temporary: &Path, destination: &Path, generated: &Generated, progress: &Progress) -> Result<()> {
    std::fs::create_dir(temporary)?;
    let filename = destination.file_name().expect("a JavaScript filename").to_string_lossy();
    let reference = format!("import({})", serde_json::to_string(&format!("./{filename}"))?);
    for path in [&generated.entrypoint, &generated.entrypoint.with_extension("d.ts")] {
        let source = if filename == "client.js" {
            generated.files[path].clone()
        } else {
            generated.files[path].replace("import('./client.js')", &reference).replace("import(\"./client.js\")", &reference)
        };
        crate::writing::write_source(&temporary.join(path), &source, progress)?;
    }
    Ok(())
}

pub(crate) fn publish(temporary: &Path, destination: &Path, overwrite: bool, progress: &Progress) -> Result<()> {
    validate(destination, overwrite)?;
    let targets = destinations(destination);
    let sources = [temporary.join("client.js"), temporary.join("client.d.ts")];
    let mut backups = Vec::new();
    let mut published = Vec::new();
    let result = (|| {
        for (index, target) in targets.iter().enumerate() {
            progress.check()?;
            if overwrite && target.exists() {
                let backup = temporary.join(format!("previous-{index}"));
                std::fs::rename(target, &backup)?;
                backups.push((target, backup));
            }
            publishing::file(&sources[index], target, false)?;
            published.push(target);
        }
        Ok(())
    })();
    if result.is_err() {
        for target in published.iter().rev() {
            std::fs::remove_file(target).report_err("remove partially published SDK bundle");
        }
        for (target, backup) in backups.iter().rev() {
            if let Err(error) = std::fs::rename(backup, target) {
                bail!(
                    "SDK publication failed: {}; restoring {} failed: {error}; backup remains in {}",
                    result.expect_err("publication failed"),
                    target.display(),
                    backup.display()
                );
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_second_file_restores_both_previous_files() {
        let directory = std::env::temp_dir().join(format!("kestrel-bundle-rollback-{}", uuid::Uuid::new_v4()));
        let temporary = directory.join("staging");
        std::fs::create_dir_all(&temporary).unwrap();
        let destination = directory.join("custom.js");
        std::fs::write(&destination, "previous JavaScript").unwrap();
        std::fs::write(destination.with_extension("d.ts"), "previous types").unwrap();
        std::fs::write(temporary.join("client.js"), "new JavaScript").unwrap();
        assert!(publish(&temporary, &destination, true, &Progress::default()).is_err());
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "previous JavaScript");
        assert_eq!(std::fs::read_to_string(destination.with_extension("d.ts")).unwrap(), "previous types");
        assert!(!temporary.join("previous-0").exists());
        assert!(!temporary.join("previous-1").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
