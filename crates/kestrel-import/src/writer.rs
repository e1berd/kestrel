use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use kestrel_core::collection::{FOLDER_FILE, sanitize_file_name};
use kestrel_core::format::{Defaults, FolderFile, Info, KeyValue};
use kestrel_core::report::ReportExt as _;
use kestrel_core::transfer::{Progress, Unit};
use kestrel_core::{Collection, Document};

use crate::{Format, PreparedImport, native};

pub(super) fn save(import: &PreparedImport, destination: &Path, name: Option<&str>, progress: &Progress) -> Result<PathBuf> {
    progress.check()?;
    let destination = std::path::absolute(destination)?;
    ensure_empty(&destination)?;
    let parent = destination.parent().context("choose a destination with a parent folder")?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".kestrel-import-{}", uuid::Uuid::new_v4()));
    let result = write(import, &staging, &destination, name, progress).and_then(|()| {
        progress.start("Publishing collection", None, Unit::Files)?;
        ensure_empty(&destination)?;
        if destination.exists() {
            std::fs::remove_dir(&destination)?;
        }
        std::fs::rename(&staging, &destination).context("move the imported collection into its destination")?;
        progress.published("Import complete", import.preview.requests as u64, Unit::Requests);
        Ok(destination)
    });
    if staging.exists() {
        std::fs::remove_dir_all(&staging).report_err("remove temporary import files");
    }
    result
}

fn ensure_empty(destination: &Path) -> Result<()> {
    if destination.exists() && (!destination.is_dir() || std::fs::read_dir(destination)?.next().transpose()?.is_some()) {
        bail!("{} is not an empty folder; choose an empty destination for the import", destination.display());
    }
    Ok(())
}

fn remembered_source(source: &str, destination: &Path) -> String {
    if source.starts_with("http://") || source.starts_with("https://") {
        return source.into();
    }
    let source = Path::new(source);
    let source_parts: Vec<_> = source.components().collect();
    let destination_parts: Vec<_> = destination.components().collect();
    let common = source_parts.iter().zip(&destination_parts).take_while(|(left, right)| left == right).count();
    if common <= 1 {
        return source.display().to_string();
    }
    let mut relative = PathBuf::new();
    for _ in common..destination_parts.len() {
        relative.push("..");
    }
    for part in &source_parts[common..] {
        relative.push(part);
    }
    relative.display().to_string()
}

fn write(import: &PreparedImport, staging: &Path, destination: &Path, name: Option<&str>, progress: &Progress) -> Result<()> {
    let mut collection = Collection::create(staging, name.unwrap_or(&import.preview.title))?;
    collection.doc.data.docs = import.collection.docs.clone();
    let mut variables = import.collection.variables.clone();
    if !variables.iter().any(|variable| variable.name == "baseUrl") {
        variables.push(KeyValue::new("baseUrl", import.base_url.as_deref().unwrap_or("http://localhost:8000")));
    }
    collection.doc.data.request = Some(Defaults { variables, ..Default::default() });
    if import.preview.format == Format::OpenApi {
        collection.doc.data.extra.insert(
            "extensions".into(),
            serde_yaml_ng::to_value(serde_json::json!({"volley": {"openapi": [{
                "sourceUrl": remembered_source(&import.source, destination), "groupBy": import.group_by.as_str()
            }]}}))?,
        );
    }
    collection.doc.save()?;
    let mut directories: IndexMap<Vec<String>, PathBuf> = IndexMap::new();
    directories.insert(Vec::new(), staging.into());
    let mut allocator = Allocator::default();
    progress.start("Creating folders", Some(import.collection.folders.len() as u64), Unit::Files)?;
    for (folders, docs) in &import.collection.folders {
        progress.check()?;
        let parent = directories.get(&folders[..folders.len() - 1]).context("the import folder has no parent")?;
        let name = folders.last().context("the import folder has no name")?;
        let directory = allocator.path(parent, name, "");
        std::fs::create_dir(&directory)?;
        let file = FolderFile {
            info: Info { name: name.clone(), kind: "folder".into(), ..Default::default() },
            docs: docs.clone(),
            ..Default::default()
        };
        Document::new(directory.join(FOLDER_FILE), file, true).save()?;
        directories.insert(folders.clone(), directory);
        progress.advance(1)?;
    }
    progress.start("Writing requests", Some(import.data.requests.len() as u64), Unit::Requests)?;
    for location in &import.data.requests {
        progress.check()?;
        let request: native::Request = import.data.store.lock().expect("prepared import store lock").read(*location)?;
        let directory = directories.get(&request.folders).context("the request folder was not imported")?;
        Document::new(allocator.path(directory, &request.file.info.name, ".yml"), request.file, true).save()?;
        progress.advance(1)?;
    }
    Ok(())
}

#[derive(Default)]
struct Allocator {
    counters: std::collections::HashMap<(PathBuf, String, String), u64>,
}

impl Allocator {
    fn path(&mut self, parent: &Path, name: &str, extension: &str) -> PathBuf {
        let name = sanitize_file_name(name);
        let next = self.counters.entry((parent.into(), name.clone(), extension.into())).or_default();
        loop {
            let suffix = if *next == 0 { String::new() } else { format!(" {next}") };
            let path = parent.join(format!("{name}{suffix}{extension}"));
            *next += 1;
            if !path.exists() {
                return path;
            }
        }
    }
}
