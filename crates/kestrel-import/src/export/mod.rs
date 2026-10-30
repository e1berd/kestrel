mod index;
mod native;
mod operation;
mod serialize;

use std::fs::OpenOptions;
use std::io::{BufWriter, Write as _};
use std::path::Path;

use anyhow::{Result, bail};
use kestrel_core::report::ReportExt as _;
use kestrel_core::transfer::{Progress, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    OpenApi30,
    OpenApi31,
    OpenApi32,
    Swagger20,
    Apidog,
}

impl ExportFormat {
    pub const ALL: [Self; 5] = [Self::OpenApi30, Self::OpenApi31, Self::OpenApi32, Self::Swagger20, Self::Apidog];
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenApi30 => "openapi30",
            Self::OpenApi31 => "openapi31",
            Self::OpenApi32 => "openapi32",
            Self::Swagger20 => "swagger20",
            Self::Apidog => "apidog",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenApi30 => "OpenAPI 3.0",
            Self::OpenApi31 => "OpenAPI 3.1",
            Self::OpenApi32 => "OpenAPI 3.2",
            Self::Swagger20 => "Swagger 2.0",
            Self::Apidog => "Apidog",
        }
    }
    pub fn version(self) -> &'static str {
        match self {
            Self::OpenApi30 => "3.0.4",
            Self::OpenApi31 => "3.1.2",
            Self::OpenApi32 => "3.2.1",
            Self::Swagger20 => "2.0",
            Self::Apidog => "1.0.0",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL.into_iter().find(|format| format.id() == value).ok_or_else(|| anyhow::anyhow!("unknown export format: {value}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Json,
    Yaml,
}

#[derive(Debug, Clone)]
pub struct ExportReport {
    pub requests: usize,
    pub bytes: u64,
    pub warnings: Vec<String>,
}

pub fn export(
    source: &Path,
    destination: &Path,
    format: ExportFormat,
    encoding: Encoding,
    overwrite: bool,
    progress: &Progress,
) -> Result<ExportReport> {
    progress.check()?;
    if format == ExportFormat::Apidog && encoding != Encoding::Json {
        bail!("Apidog exports must use JSON");
    }
    if destination.exists() && !overwrite {
        bail!("{} already exists; choose another file or allow replacement", destination.display());
    }
    let source = std::path::absolute(source)?;
    let destination = std::path::absolute(destination)?;
    if destination.starts_with(&source) && destination.extension().is_some_and(|extension| extension == "yml" || extension == "yaml") {
        bail!("save YAML exports outside the collection so they cannot replace or be mistaken for request files");
    }
    if destination == source.join(kestrel_core::collection::COLLECTION_FILE)
        || destination.starts_with(source.join("environments"))
        || destination.file_name().is_some_and(|name| name == ".env" || name == "folder.yml")
    {
        bail!("choose an export destination outside collection metadata and environment files");
    }
    let index = index::Index::read(&source, &destination, format, progress)?;
    let parent = destination.parent().ok_or_else(|| anyhow::anyhow!("the export destination has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".kestrel-export-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        let mut writer = BufWriter::with_capacity(64 * 1024, progress.writer(file));
        progress.start("Writing export", Some(index.requests.len() as u64), Unit::Requests)?;
        let document = serialize::Document { index: &index, format, progress };
        match encoding {
            Encoding::Json => serde_json::to_writer(&mut writer, &document)?,
            Encoding::Yaml => serde_yaml_ng::to_writer(&mut writer, &document)?,
        }
        writer.flush()?;
        writer.get_ref().get_ref().sync_all()?;
        let bytes = writer.get_ref().get_ref().metadata()?.len();
        drop(writer);
        progress.start("Publishing export", None, Unit::Files)?;
        if destination.exists() && !overwrite {
            bail!("the export destination already exists");
        }
        if overwrite {
            std::fs::rename(&temporary, &destination)?;
        } else {
            std::fs::hard_link(&temporary, &destination)?;
        }
        progress.published("Export complete", index.requests.len() as u64, Unit::Requests);
        Ok(ExportReport { requests: index.requests.len(), bytes, warnings: index.warnings })
    })();
    if temporary.exists() {
        std::fs::remove_file(&temporary).report_err("remove unfinished export");
    }
    result
}
