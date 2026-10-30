use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use serde_json::Value as Json;
use url::Url;

pub(super) fn load(source: &str, base: &Path) -> Result<Json> {
    let url = if source.starts_with("https://") || source.starts_with("http://") {
        Url::parse(source)?
    } else {
        let path = std::path::absolute(base.join(source))?;
        Url::from_file_path(&path).map_err(|()| anyhow::anyhow!("invalid spec path: {}", path.display()))?
    };
    let mut documents = IndexMap::new();
    documents.insert(url.to_string(), Json::Null);
    let mut root = read(&url)?;
    rewrite(&mut root, &url, 0, &mut documents, 0)?;
    if documents.len() > 1 {
        root["x-kestrel-documents"] = Json::Array(documents.into_values().collect());
    }
    Ok(root)
}

fn read(source: &Url) -> Result<Json> {
    let text = match source.scheme() {
        "file" => {
            let path = source.to_file_path().map_err(|()| anyhow::anyhow!("invalid file URL: {source}"))?;
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?
        }
        "http" | "https" => reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?
            .get(source.clone())
            .send()
            .with_context(|| format!("download {source}"))?
            .error_for_status()?
            .text()?,
        scheme => bail!("unsupported OpenAPI reference scheme: {scheme}"),
    };
    serde_yaml_ng::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("parse {source} as JSON or YAML"))
}

fn rewrite(value: &mut Json, source: &Url, document: usize, documents: &mut IndexMap<String, Json>, depth: usize) -> Result<()> {
    if depth > 128 {
        bail!("OpenAPI references are nested more than 128 levels deep");
    }
    match value {
        Json::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Json::as_str).map(str::to_string) {
                if let Some(pointer) = reference.strip_prefix('#') {
                    if document > 0 {
                        if !pointer.is_empty() && !pointer.starts_with('/') {
                            bail!("OpenAPI schema anchors are not supported: {reference}");
                        }
                        object.insert("$ref".into(), Json::String(format!("#/x-kestrel-documents/{document}{pointer}")));
                    }
                } else {
                    let mut target = source.join(&reference).with_context(|| format!("resolve {reference} from {source}"))?;
                    let fragment = target.fragment().unwrap_or_default().to_string();
                    target.set_fragment(None);
                    if !fragment.is_empty() && !fragment.starts_with('/') {
                        bail!("OpenAPI schema anchors are not supported: {reference}");
                    }
                    let index = match documents.get_index_of(target.as_str()) {
                        Some(index) => index,
                        None => {
                            if documents.len() >= 128 {
                                bail!("OpenAPI references more than 128 external documents");
                            }
                            let index = documents.len();
                            documents.insert(target.to_string(), Json::Null);
                            let mut external = read(&target)?;
                            rewrite(&mut external, &target, index, documents, depth + 1)?;
                            documents.insert(target.to_string(), external);
                            index
                        }
                    };
                    let prefix = if index == 0 { String::new() } else { format!("/x-kestrel-documents/{index}") };
                    object.insert("$ref".into(), Json::String(format!("#{prefix}{fragment}")));
                }
            }
            for child in object.values_mut() {
                rewrite(child, source, document, documents, depth + 1)?;
            }
        }
        Json::Array(array) => {
            for child in array {
                rewrite(child, source, document, documents, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
