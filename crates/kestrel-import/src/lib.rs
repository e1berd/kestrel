use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use kestrel_core::Collection;
use kestrel_core::transfer::Progress;
use kestrel_openapi::GroupBy;

mod apidog;
pub mod export;
mod format;
mod input;
mod native;
mod preparing;
mod storage;
mod writer;

pub use format::Format;

#[derive(Debug, Clone)]
pub struct Preview {
    pub format: Format,
    pub title: String,
    pub requests: usize,
    pub folders: usize,
    pub warnings: Vec<String>,
    pub request_data_bytes: u64,
}

impl Preview {
    pub fn open_after_import_by_default(&self) -> bool {
        self.requests <= 5000 && self.request_data_bytes <= 128 * 1024 * 1024
    }
}

#[derive(Debug)]
struct PreparedData {
    requests: Vec<storage::Location>,
    store: Mutex<storage::Store>,
    _directory: storage::TemporaryDirectory,
}

#[derive(Debug, Clone)]
pub struct PreparedImport {
    pub preview: Preview,
    collection: native::Collection,
    data: Arc<PreparedData>,
    source: String,
    group_by: GroupBy,
    base_url: Option<String>,
}

impl PreparedImport {
    pub fn load(source: &str, base: &Path, format: Option<Format>, group_by: GroupBy) -> Result<Self> {
        Self::load_with_progress(source, base, format, group_by, &Progress::default())
    }

    pub fn load_with_progress(source: &str, base: &Path, format: Option<Format>, group_by: GroupBy, progress: &Progress) -> Result<Self> {
        preparing::load(source, base, format, group_by, progress)
    }

    pub fn parse(text: &str, source: &str, format: Option<Format>, group_by: GroupBy) -> Result<Self> {
        let directory = storage::TemporaryDirectory::new()?;
        let path = directory.0.join("input");
        std::fs::write(&path, text)?;
        let mut prepared = Self::load(&path.to_string_lossy(), Path::new("/"), format, group_by)?;
        prepared.source = source.into();
        Ok(prepared)
    }

    pub fn save(&self, destination: &Path, name: Option<&str>) -> Result<Collection> {
        Collection::open(&self.save_with_progress(destination, name, &Progress::default())?)
    }

    pub fn save_with_progress(&self, destination: &Path, name: Option<&str>, progress: &Progress) -> Result<std::path::PathBuf> {
        writer::save(self, destination, name, progress)
    }
}
