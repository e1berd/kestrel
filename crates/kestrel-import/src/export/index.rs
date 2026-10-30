use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use kestrel_core::collection::{COLLECTION_FILE, FOLDER_FILE};
use kestrel_core::format::{Auth, CollectionFile, Defaults, FolderFile, RequestFile};
use kestrel_core::transfer::{Progress, Unit};

use super::ExportFormat;

pub(super) struct Entry {
    pub file: PathBuf,
    pub folders: Vec<String>,
    pub method: String,
    pub defaults: Arc<Defaults>,
}

pub(super) struct Index {
    pub collection: CollectionFile,
    pub requests: Vec<Entry>,
    pub paths: BTreeMap<String, Vec<usize>>,
    pub warnings: Vec<String>,
    pub tree: BTreeMap<Vec<String>, Node>,
    pub folders: BTreeMap<Vec<String>, PathBuf>,
    pub swagger_base: Option<String>,
}

#[derive(Default)]
pub(super) struct Node {
    pub requests: Vec<usize>,
    pub children: std::collections::BTreeSet<String>,
}

impl Index {
    pub fn read(root: &Path, destination: &Path, format: ExportFormat, progress: &Progress) -> Result<Self> {
        let collection: CollectionFile = read(&root.join(COLLECTION_FILE))?;
        let mut files = Vec::new();
        let mut folders = BTreeMap::new();
        progress.start("Finding collection files", None, Unit::Files)?;
        discover(
            root,
            root,
            destination,
            &collection.ignored(),
            &[],
            Arc::new(collection.request.clone().unwrap_or_default()),
            &mut files,
            &mut folders,
            progress,
        )?;
        let total: u64 = files
            .iter()
            .map(|(path, _, _)| std::fs::metadata(path).map(|metadata| metadata.len()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .sum();
        progress.start("Indexing saved requests", Some(total), Unit::Bytes)?;
        let mut index = Self {
            collection,
            requests: Vec::new(),
            paths: BTreeMap::new(),
            warnings: Vec::new(),
            tree: BTreeMap::new(),
            folders,
            swagger_base: None,
        };
        for path in index.folders.keys() {
            index.tree.entry(path.clone()).or_default();
            for depth in 0..path.len() {
                index.tree.entry(path[..depth].to_vec()).or_default().children.insert(path[depth].clone());
            }
        }
        for (file, folders, defaults) in files {
            progress.check()?;
            let request: RequestFile = serde_yaml_ng::from_reader(BufReader::new(progress.reader(File::open(&file)?)))?;
            let Some(http) = &request.http else { continue };
            if !request.scripts().is_empty() {
                index.warnings.push("Scripts and tests are not exported because their runtime APIs differ.".into());
            }
            let mut route = route(&http.url)?;
            if format == ExportFormat::Swagger20 {
                if index
                    .collection
                    .request
                    .as_ref()
                    .into_iter()
                    .flat_map(|defaults| &defaults.variables)
                    .any(|variable| variable.name == "baseUrl" && variable.is_secret())
                {
                    bail!("Swagger 2.0 requires a public base URL; use OpenAPI or Apidog to preserve an environment placeholder");
                }
                let configured = index
                    .collection
                    .request
                    .as_ref()
                    .into_iter()
                    .flat_map(|defaults| &defaults.variables)
                    .find(|variable| variable.name == "baseUrl" && !variable.disabled)
                    .map(|variable| variable.value.as_str());
                let base = if http.url.starts_with("{{") {
                    configured.unwrap_or("http://localhost:8000").to_string()
                } else {
                    let url = url::Url::parse(&http.url).context("export a Swagger request URL")?;
                    if let Some(base) = configured.filter(|base| http.url.strip_prefix(*base).is_some_and(|suffix| suffix.starts_with('/')))
                    {
                        let parsed = url::Url::parse(base)?;
                        let prefix = parsed.path().trim_end_matches('/');
                        if !prefix.is_empty() {
                            route = route.strip_prefix(prefix).context("derive the Swagger base path")?.to_string();
                        }
                        base.to_string()
                    } else {
                        url.origin().ascii_serialization()
                    }
                };
                if index.swagger_base.as_ref().is_some_and(|existing| *existing != base) {
                    bail!("Swagger 2.0 cannot represent requests with different base URLs; use OpenAPI 3.2 or Apidog");
                }
                index.swagger_base = Some(base);
            }
            let method = http.method.to_ascii_lowercase();
            let standard = matches!(method.as_str(), "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace");
            if !standard && !matches!(format, ExportFormat::OpenApi32 | ExportFormat::Apidog) {
                bail!("{} cannot represent HTTP method {}; use OpenAPI 3.2 or Apidog", format.name(), http.method);
            }
            let entries = index.paths.entry(route.clone()).or_default();
            if format != ExportFormat::Apidog && entries.iter().any(|index_id| index.requests[*index_id].method == method) {
                bail!(
                    "{} cannot contain duplicate {} {} requests; use Apidog or give them distinct paths",
                    format.name(),
                    http.method,
                    route
                );
            }
            entries.push(index.requests.len());
            for depth in 0..folders.len() {
                index.tree.entry(folders[..depth].to_vec()).or_default().children.insert(folders[depth].clone());
            }
            index.tree.entry(folders.clone()).or_default().requests.push(index.requests.len());
            index.requests.push(Entry { file, folders, method, defaults });
        }
        if index.requests.is_empty() {
            bail!("the collection has no HTTP requests to export");
        }
        if root.join("environments").is_dir() {
            index.warnings.push("Environments are not exported; process.env placeholders are preserved.".into());
        }
        index.warnings.push("Authentication credentials are exported as process.env placeholders.".into());
        index.warnings.sort();
        index.warnings.dedup();
        Ok(index)
    }
}

pub(super) fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_yaml_ng::from_reader(BufReader::new(File::open(path)?)).with_context(|| format!("read {}", path.display()))
}

#[allow(clippy::too_many_arguments)]
fn discover(
    root: &Path,
    directory: &Path,
    destination: &Path,
    ignored: &[String],
    folders: &[String],
    defaults: Arc<Defaults>,
    files: &mut Vec<(PathBuf, Vec<String>, Arc<Defaults>)>,
    folder_files: &mut BTreeMap<Vec<String>, PathBuf>,
    progress: &Progress,
) -> Result<()> {
    progress.check()?;
    let mut entries = std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type()?.is_symlink()
            || name.starts_with('.')
            || name == "environments"
            || name == "node_modules"
            || ignored.iter().any(|ignore| ignore == &name || Path::new(ignore) == path.strip_prefix(root).unwrap_or(&path))
        {
            continue;
        }
        if path.is_dir() {
            let folder: FolderFile = if path.join(FOLDER_FILE).is_file() { read(&path.join(FOLDER_FILE))? } else { FolderFile::default() };
            let mut ancestors = folders.to_vec();
            ancestors.push(if folder.info.name.is_empty() { name } else { folder.info.name });
            folder_files.insert(ancestors.clone(), path.join(FOLDER_FILE));
            let mut inherited = (*defaults).clone();
            if let Some(own) = folder.request {
                if own.auth.as_ref().is_some_and(|auth| auth.mode() != "inherit") {
                    inherited.auth = own.auth;
                }
                inherited.headers.extend(own.headers);
                inherited.variables.extend(own.variables);
            }
            discover(root, &path, destination, ignored, &ancestors, Arc::new(inherited), files, folder_files, progress)?;
        } else if path != destination
            && !matches!(name.as_str(), COLLECTION_FILE | FOLDER_FILE)
            && path.extension().is_some_and(|extension| extension == "yml" || extension == "yaml")
        {
            files.push((path, folders.to_vec(), defaults.clone()));
            progress.advance(1)?;
        }
    }
    Ok(())
}

fn route(url: &str) -> Result<String> {
    let path = if url.starts_with("{{") {
        let end = url.find("}}").context("the request URL has an unclosed variable")?;
        &url[end + 2..]
    } else if url.contains("://") {
        let start = url.find("://").expect("a URL scheme") + 3;
        &url[url[start..].find('/').map(|offset| start + offset).unwrap_or(url.len())..]
    } else {
        url
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = if path.is_empty() { "/" } else { path };
    if !path.starts_with('/') {
        bail!("cannot derive an API path from URL {url}");
    }
    Ok(path
        .split('/')
        .map(|segment| segment.strip_prefix(':').map(|name| format!("{{{name}}}")).unwrap_or_else(|| segment.into()))
        .collect::<Vec<_>>()
        .join("/"))
}

pub(super) fn auth(request: &RequestFile, entry: &Entry) -> Auth {
    request
        .http
        .as_ref()
        .and_then(|http| http.auth.clone())
        .filter(|auth| auth.mode() != "inherit")
        .or_else(|| entry.defaults.auth.clone())
        .unwrap_or(Auth::Mode("none".into()))
}
