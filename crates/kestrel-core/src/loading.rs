use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use indexmap::IndexMap;

use crate::collection::{COLLECTION_FILE, Environment, FOLDER_FILE, Folder, Item, Request};
use crate::document::Document;
use crate::format::{FolderFile, RequestFile};

pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = if start.is_file() { start.parent()? } else { start };
    let start = start.canonicalize().ok()?;
    start.ancestors().find(|dir| dir.join(COLLECTION_FILE).is_file()).map(Path::to_path_buf)
}
pub(crate) fn read_dir_tracked(dir: &Path, ignored: &HashSet<String>, progress: Option<&crate::transfer::Progress>) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.path())).collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if let Some(progress) = progress {
            progress.check()?;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') || ignored.contains(&name) {
            continue;
        }
        if path.is_dir() {
            let folder_file = path.join(FOLDER_FILE);
            let doc = if folder_file.is_file() {
                let doc = Document::read(&folder_file, true)?;
                if let Some(progress) = progress {
                    progress.advance(1)?;
                }
                doc
            } else {
                Document::new(folder_file, FolderFile::default(), true)
            };
            let children = read_dir_tracked(&path, ignored, progress)?;
            items.push(Item::Folder(Folder { dir: path, doc, children }));
        } else if is_request_file(&path) {
            let doc = Document::<RequestFile>::read(&path, true)?;
            if let Some(progress) = progress {
                progress.advance(1)?;
            }
            if doc.data.info.kind.is_empty() || doc.data.info.kind == "http" || doc.data.http.is_some() {
                items.push(Item::Request(Request { doc }));
            }
        }
    }
    items.sort_by_cached_key(|item| (matches!(item, Item::Request(_)), item.seq(), item.name().to_lowercase()));
    Ok(items)
}
pub(crate) fn is_request_file(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    matches!(path.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")) && name != FOLDER_FILE && name != COLLECTION_FILE
}
pub(crate) fn read_environments(dir: &Path) -> Result<Vec<Environment>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")))
        .collect();
    paths.sort();
    paths.into_iter().map(|path| Ok(Environment { doc: Document::read(&path, false)? })).collect()
}
pub(crate) fn read_dotenv(path: &Path) -> IndexMap<String, String> {
    match dotenvy::from_path_iter(path) {
        Ok(iter) => iter.filter_map(|e| e.ok()).collect(),
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => IndexMap::new(),
        Err(error) => {
            crate::report::report_error(&format!("read {}", path.display()), error);
            IndexMap::new()
        }
    }
}
pub fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '-' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() { "untitled".into() } else { cleaned }
}
pub(crate) fn unique_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let make = |n: usize| {
        let stem = if n == 0 { stem.to_string() } else { format!("{stem} {n}") };
        if extension.is_empty() { dir.join(stem) } else { dir.join(format!("{stem}.{extension}")) }
    };
    (0..).map(make).find(|p| !p.exists()).expect("a free name")
}
