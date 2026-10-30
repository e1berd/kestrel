use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::GroupBy;
use crate::generate::{generate, request_key};
use anyhow::{Context as _, Result};
use indexmap::IndexMap;
use kestrel_core::collection::{FOLDER_FILE, file_stem, sanitize_file_name};
use kestrel_core::format::{Auth, Docs, FolderFile, Info, KeyValue, RequestFile};
use kestrel_core::{Collection, Document, Item};
use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: usize,
    pub stale: Vec<String>,
    pub removed: Vec<String>,
}
impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} added, {} updated, {} unchanged", self.added.len(), self.updated.len(), self.unchanged)?;
        if !self.stale.is_empty() {
            write!(f, ", {} no longer in the spec", self.stale.len())?;
        }
        if !self.removed.is_empty() {
            write!(f, ", {} removed", self.removed.len())?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub group_by: GroupBy,
    pub prune: bool,
    pub dry_run: bool,
}
pub fn sync(collection: &mut Collection, spec: &Json, options: &SyncOptions) -> Result<Report> {
    let generated = generate(spec, options.group_by);
    let mut report = Report::default();
    let mut candidates: IndexMap<String, Vec<(PathBuf, String, Vec<String>)>> = IndexMap::new();
    for request in collection.requests() {
        if let Some(key) = request_key(&request.doc.data) {
            let info = &request.doc.data.info;
            candidates.entry(key).or_default().push((request.doc.path.clone(), info.name.clone(), info.tags.clone()));
        }
    }
    let mut existing: IndexMap<String, PathBuf> = IndexMap::new();
    for op in &generated.operations {
        let Some(list) = candidates.get(&op.key) else {
            continue;
        };
        let by_name = list.iter().find(|(_, name, _)| *name == op.file.info.name);
        let by_tags = list.iter().find(|(_, _, tags)| !tags.is_empty() && *tags == op.file.info.tags);
        if let Some((path, _, _)) = by_name.or(by_tags) {
            existing.insert(op.key.clone(), path.clone());
        }
    }
    let generated_keys: HashSet<&String> = generated.operations.iter().map(|op| &op.key).collect();
    let mut stale_candidates: Vec<PathBuf> = Vec::new();
    for (key, list) in &candidates {
        for (path, _, tags) in list {
            let matched = existing.get(key) == Some(path);
            if !matched && !tags.is_empty() && !generated_keys.contains(key) {
                stale_candidates.push(path.clone());
            }
        }
    }
    let mut created_folders: IndexMap<String, PathBuf> = IndexMap::new();
    for op in &generated.operations {
        let label = format!("{} {}", op.file.http().method, op.file.info.name);
        if let Some(path) = existing.get(&op.key) {
            let request = collection.request_mut(path).context("the request went away")?;
            let mut doc = request.doc.clone();
            refresh(&mut doc.data, &op.file);
            if doc.is_dirty() {
                report.updated.push(label);
                if !options.dry_run {
                    doc.save()?;
                    request.doc = doc;
                }
            } else {
                report.unchanged += 1;
            }
            continue;
        }
        report.added.push(label);
        if options.dry_run {
            continue;
        }
        let dir = match &op.folder {
            None => collection.root.clone(),
            Some(name) => match created_folders.get(name) {
                Some(dir) => dir.clone(),
                None => {
                    let dir = folder_dir(collection, name, generated.folder_docs.get(name))?;
                    created_folders.insert(name.clone(), dir.clone());
                    dir
                }
            },
        };
        let path = unique_file(&dir, &op.file.info.name);
        Document::new(path, op.file.clone(), true).save()?;
    }
    for path in &stale_candidates {
        let label = collection.request(path).map(|r| format!("{} {}", r.method(), r.name())).unwrap_or_else(|| path.display().to_string());
        if options.prune && !options.dry_run {
            std::fs::remove_file(path)?;
            report.removed.push(label);
        } else {
            report.stale.push(label);
        }
    }
    if !options.dry_run {
        collection.reload()?;
    }
    Ok(report)
}
pub(crate) fn refresh(file: &mut RequestFile, generated: &RequestFile) {
    file.info.name = generated.info.name.clone();
    file.info.tags = generated.info.tags.clone();
    if generated.docs.is_some() {
        file.docs = generated.docs.clone();
    }
    match generated.extra.get("examples") {
        Some(examples) => {
            file.extra.insert("examples".into(), examples.clone());
        }
        None => {
            file.extra.shift_remove("examples");
        }
    }
    let new_http = generated.http();
    let http = file.http.get_or_insert_with(Default::default);
    let old = std::mem::take(&mut http.params);
    let mut used = vec![false; old.len()];
    let mut take = |p: &KeyValue, same_value: bool| {
        let i = old
            .iter()
            .enumerate()
            .position(|(i, o)| !used[i] && o.name == p.name && o.kind == p.kind && (!same_value || o.value == p.value))?;
        used[i] = true;
        Some(old[i].clone())
    };
    let mut params = Vec::new();
    for p in &new_http.params {
        let kept = take(p, true).or_else(|| take(p, false));
        params.push(match kept {
            Some(o) => KeyValue { description: p.description.clone(), ..o },
            None => p.clone(),
        });
    }
    for (i, o) in old.iter().enumerate() {
        if !used[i] && o.kind.as_deref() == Some("query") && !o.value.is_empty() && !new_http.params.iter().any(|p| p.name == o.name) {
            params.push(o.clone());
        }
    }
    http.params = params;
    if http.body.is_none() || http.body.as_ref().is_some_and(|b| b.as_text().trim().is_empty() && !b.is_form()) {
        http.body = new_http.body.clone();
    }
    for header in &new_http.headers {
        if !http.headers.iter().any(|h| h.name.eq_ignore_ascii_case(&header.name)) {
            http.headers.push(header.clone());
        }
    }
}
pub(crate) fn folder_dir(collection: &Collection, name: &str, docs: Option<&String>) -> Result<PathBuf> {
    for item in &collection.items {
        if let Item::Folder(folder) = item
            && (folder.name() == name || file_stem(&folder.dir) == sanitize_file_name(name))
        {
            return Ok(folder.dir.clone());
        }
    }
    let dir = collection.root.join(sanitize_file_name(name));
    std::fs::create_dir_all(&dir)?;
    let file = FolderFile {
        info: Info { name: name.into(), kind: "folder".into(), ..Default::default() },
        request: Some(kestrel_core::format::Defaults { auth: Some(Auth::Mode("inherit".into())), ..Default::default() }),
        docs: docs.map(|d| Docs::Rich { content: d.clone(), kind: Some("text/markdown".into()) }),
        ..Default::default()
    };
    Document::new(dir.join(FOLDER_FILE), file, true).save()?;
    Ok(dir)
}
pub(crate) fn unique_file(dir: &Path, name: &str) -> PathBuf {
    let stem = sanitize_file_name(name);
    (0..)
        .map(|n| if n == 0 { dir.join(format!("{stem}.yml")) } else { dir.join(format!("{stem} {n}.yml")) })
        .find(|p| !p.exists())
        .expect("a free name")
}
pub fn import(dir: &Path, spec_source: &str, spec: &Json, group_by: GroupBy, name: Option<&str>) -> Result<(Collection, Report)> {
    let generated = generate(spec, group_by);
    let name = name.map(str::to_string).unwrap_or_else(|| generated.title.clone());
    let mut collection = Collection::create(dir, &name)?;
    let mut ext = serde_yaml_ng::Mapping::new();
    let mut source = serde_yaml_ng::Mapping::new();
    source.insert("sourceUrl".into(), spec_source.into());
    source.insert("groupBy".into(), group_by.as_str().into());
    ext.insert("openapi".into(), Yaml::Sequence(vec![Yaml::Mapping(source)]));
    let mut extensions = serde_yaml_ng::Mapping::new();
    extensions.insert("volley".into(), Yaml::Mapping(ext));
    collection.doc.data.extra.insert("extensions".into(), Yaml::Mapping(extensions));
    if let Some(description) = spec.pointer("/info/description").and_then(Json::as_str) {
        collection.doc.data.docs = Some(Docs::Rich { content: description.into(), kind: Some("text/markdown".into()) });
    }
    let base_url = generated.base_url.clone().unwrap_or_else(|| "http://localhost:8000".into());
    collection.doc.data.request =
        Some(kestrel_core::format::Defaults { variables: vec![KeyValue::new("baseUrl", base_url)], ..Default::default() });
    collection.doc.save()?;
    let report = sync(&mut collection, spec, &SyncOptions { group_by, prune: false, dry_run: false })?;
    Ok((collection, report))
}
