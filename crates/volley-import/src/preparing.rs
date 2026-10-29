use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use serde_json::{Value as Json, json};
use volley_core::format::Docs;
use volley_core::transfer::{Progress, Unit};
use volley_openapi::GroupBy;

use crate::input::documents::Documents;
use crate::storage::{Store, TemporaryDirectory};
use crate::{Format, PreparedData, PreparedImport, Preview, apidog, native};

pub(super) fn load(source: &str, base: &Path, selected: Option<Format>, group_by: GroupBy, progress: &Progress) -> Result<PreparedImport> {
    let mut documents = Documents::load(source, base, progress)?;
    let format = selected.map(Ok).unwrap_or_else(|| Format::detect(&documents.inputs[0].metadata))?;
    let directory = TemporaryDirectory::new()?;
    let mut store = Store::new(&directory.0.join("requests"))?;
    let mut locations = Vec::new();
    let mut warnings = Vec::new();
    let variables = documents.inputs[0]
        .metadata
        .get("x-kestrel-variables")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .unwrap_or_default();
    let mut collection = native::Collection { title: String::new(), docs: None, folders: IndexMap::new(), variables };
    let base_url = if format == Format::OpenApi {
        let metadata = volley_openapi::normalize(documents.inputs[0].metadata.clone())?;
        let header = volley_openapi::generate(&metadata, group_by);
        collection.title = header.title;
        collection.docs = metadata
            .pointer("/info/description")
            .and_then(Json::as_str)
            .map(|description| Docs::Rich { content: description.into(), kind: Some("text/markdown".into()) });
        if metadata.get("webhooks").is_some() {
            warnings.push("Webhook definitions are not imported as outgoing requests.".into());
        }
        let paths = documents.inputs[0].paths.clone();
        let total = paths.len() as u64;
        let mut sequences = BTreeMap::new();
        for (index, path) in paths.iter().enumerate() {
            progress.check()?;
            let mut item = documents.path(path)?;
            let mut context = documents.context(&mut item)?;
            context["paths"] = json!({path: item});
            let context = volley_openapi::normalize(context)?;
            let generated = volley_openapi::generate(&context, group_by);
            for operation in generated.operations {
                let folders: Vec<_> = operation.folder.into_iter().collect();
                if !folders.is_empty() {
                    let docs = generated
                        .folder_docs
                        .get(&folders[0])
                        .map(|content| Docs::Rich { content: content.clone(), kind: Some("text/markdown".into()) });
                    collection.folders.entry(folders.clone()).or_insert(docs);
                }
                let mut file = operation.file;
                let sequence = sequences.entry(folders.clone()).or_insert(0);
                *sequence += 1;
                file.info.seq = Some(*sequence);
                locations.push(store.append(&native::Request { folders, file })?);
            }
            progress.report("Converting OpenAPI paths", Some(total), index as u64 + 1, Unit::Paths)?;
        }
        header.base_url
    } else {
        let metadata = &documents.inputs[0].metadata;
        let version =
            metadata.get("apidogProject").and_then(Json::as_str).context("not an Apidog project export: missing `apidogProject`")?;
        if !matches!(version, "1.0.0" | "1.1.0") {
            bail!("Apidog project version {version} is not supported");
        }
        collection.title = metadata
            .pointer("/info/name")
            .or_else(|| metadata.pointer("/info/title"))
            .or_else(|| metadata.get("name"))
            .and_then(Json::as_str)
            .unwrap_or("Apidog collection")
            .into();
        collection.docs = apidog::docs(&metadata["info"]);
        for (key, label) in [
            ("environments", "Environments"),
            ("testSuiteCollection", "Test suites"),
            ("testCaseCollection", "Test scenarios"),
            ("apiTestCaseCollection", "Endpoint test cases"),
            ("webSocketCollection", "WebSocket endpoints"),
            ("socketIOCollection", "Socket.IO endpoints"),
            ("docCollection", "Standalone documents"),
            ("securitySchemeCollection", "Reusable security schemes"),
        ] {
            if metadata.get(key).is_some() {
                warnings.push(format!("{label} are not imported."));
            }
        }
        for index in 0..documents.inputs[0].folders.len() {
            if documents.inputs[0].folders[index].is_some() {
                let chain = documents.inputs[0].folder_chain(Some(index));
                let folders = folder_names(&chain);
                if !folders.is_empty() {
                    collection.folders.entry(folders).or_insert_with(|| apidog::docs(chain.last().expect("a folder")));
                }
            }
        }
        let requests = documents.inputs[0].requests.clone();
        let total = requests.len() as u64;
        for (index, (parent, location)) in requests.into_iter().enumerate() {
            progress.check()?;
            let mut item: Json = documents.inputs[0].store.read(location)?;
            let mut context = documents.context(&mut item)?;
            context["openapi"] = json!("3.1.0");
            context["info"] = json!({"title": collection.title});
            let api = item.get("api").unwrap_or(&item);
            if api.get("method").is_none() || api.get("path").is_none() {
                warnings.push("Non-HTTP items in the Apidog export are not imported.".into());
                progress.report("Converting Apidog requests", Some(total), index as u64 + 1, Unit::Requests)?;
                continue;
            }
            let chain = documents.inputs[0].folder_chain(parent);
            let mut auth = None;
            for folder in &chain {
                apidog::warn_processors(folder, &mut warnings);
                auth = apidog::request::auth(&folder["auth"], &mut warnings).or(auth);
            }
            apidog::warn_processors(api, &mut warnings);
            let mut file = apidog::request::convert(&item, api, &context, &mut warnings)?;
            file.info.seq = Some(index as i64 + 1);
            if let Some(http) = &mut file.http {
                http.auth = apidog::request::auth(&api["auth"], &mut warnings).or(auth);
            }
            locations.push(store.append(&native::Request { folders: folder_names(&chain), file })?);
            progress.report("Converting Apidog requests", Some(total), index as u64 + 1, Unit::Requests)?;
        }
        documents.inputs[0].metadata.get("x-kestrel-base-url").and_then(Json::as_str).map(str::to_string)
    };
    if locations.is_empty() {
        bail!("the specification has no HTTP operations to import");
    }
    warnings.sort();
    warnings.dedup();
    let preview = Preview {
        format,
        title: collection.title.clone(),
        requests: locations.len(),
        folders: collection.folders.len(),
        warnings,
        request_data_bytes: store.len(),
    };
    progress.finish()?;
    let source = if source.contains("://") { source.into() } else { std::path::absolute(base.join(source))?.to_string_lossy().into() };
    Ok(PreparedImport {
        preview,
        collection,
        source,
        group_by,
        base_url,
        data: Arc::new(PreparedData { requests: locations, store: Mutex::new(store), _directory: directory }),
    })
}

fn folder_names(chain: &[&Json]) -> Vec<String> {
    chain
        .iter()
        .filter_map(|folder| folder.get("name").and_then(Json::as_str))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}
