use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use serde_json::{Value as Json, json};
use url::Url;
use volley_core::transfer::{Progress, Unit};

use super::{Input, escape};
use crate::storage::TemporaryDirectory;

pub(crate) struct Documents {
    pub inputs: Vec<Input>,
    urls: Vec<Url>,
    downloaded: TemporaryDirectory,
    progress: Progress,
}

impl Documents {
    pub fn load(source: &str, base: &Path, progress: &Progress) -> Result<Self> {
        let url = if source.starts_with("http://") || source.starts_with("https://") {
            Url::parse(source)?
        } else {
            Url::from_file_path(std::path::absolute(base.join(source))?).map_err(|()| anyhow::anyhow!("invalid source path"))?
        };
        let mut documents =
            Self { inputs: Vec::new(), urls: Vec::new(), downloaded: TemporaryDirectory::new()?, progress: progress.clone() };
        documents.load_url(url)?;
        Ok(documents)
    }

    fn load_url(&mut self, url: Url) -> Result<usize> {
        if let Some(index) = self.urls.iter().position(|existing| *existing == url) {
            return Ok(index);
        }
        if self.inputs.len() >= 128 {
            bail!("the specification references more than 128 documents");
        }
        let path = match url.scheme() {
            "file" => url.to_file_path().map_err(|()| anyhow::anyhow!("invalid file reference: {url}"))?,
            "http" | "https" => {
                let response = reqwest::blocking::Client::builder()
                    .timeout(None)
                    .connect_timeout(Duration::from_secs(30))
                    .build()?
                    .get(url.clone())
                    .send()?
                    .error_for_status()?;
                self.progress.start("Downloading specification", response.content_length(), Unit::Bytes)?;
                let path = self.downloaded.0.join(self.inputs.len().to_string());
                std::io::copy(&mut self.progress.reader(response), &mut File::create(&path)?)?;
                path
            }
            scheme => bail!("unsupported specification URL scheme: {scheme}"),
        };
        let input = Input::read(&path, &self.progress)?;
        let index = self.inputs.len();
        self.inputs.push(input);
        self.urls.push(url);
        Ok(index)
    }

    pub fn context(&mut self, value: &mut Json) -> Result<Json> {
        let metadata = &self.inputs[0].metadata;
        let mut context = json!({"info": {"title": metadata.pointer("/info/title").cloned().unwrap_or(json!("API"))}});
        for key in ["openapi", "swagger", "servers", "security", "consumes", "produces", "host", "basePath", "schemes"] {
            if let Some(value) = metadata.get(key) {
                context[key] = value.clone();
            }
        }
        let tags: HashSet<_> = value
            .as_object()
            .into_iter()
            .flat_map(|object| object.values())
            .flat_map(|operation| operation.get("tags").and_then(Json::as_array).into_iter().flatten().filter_map(Json::as_str))
            .collect();
        let mut definitions = Vec::new();
        for tag in tags {
            if let Some(definition) = self.inputs[0].node(&format!("/tags/{}", escape(tag)))? {
                definitions.push(definition);
            }
        }
        context["tags"] = definitions.into();
        context["paths"] = json!({});
        let mut schemes = Vec::new();
        for prefix in ["/components/securitySchemes/", "/securityDefinitions/"] {
            schemes.extend(
                self.inputs[0]
                    .nodes
                    .range(prefix.to_string()..)
                    .take_while(|(pointer, _)| pointer.starts_with(prefix))
                    .map(|(pointer, _)| pointer.clone()),
            );
        }
        let mut seen = HashSet::new();
        for pointer in schemes {
            let mut scheme = self.inputs[0].node(&pointer)?.context("missing security scheme")?;
            self.expand(&mut scheme, 0, &mut context, &mut seen, 0)?;
            insert(&mut context, &pointer, scheme)?;
        }
        self.expand(value, 0, &mut context, &mut seen, 0)?;
        Ok(context)
    }

    fn expand(
        &mut self,
        value: &mut Json,
        document: usize,
        context: &mut Json,
        seen: &mut HashSet<(usize, String)>,
        depth: usize,
    ) -> Result<()> {
        self.progress.check()?;
        if depth > 128 {
            bail!("specification references are nested more than 128 levels deep");
        }
        match value {
            Json::Object(object) => {
                if let Some(reference) = object.get("$ref").and_then(Json::as_str).map(str::to_string) {
                    let (index, pointer) = if let Some(pointer) = reference.strip_prefix('#') {
                        (document, pointer.to_string())
                    } else {
                        let mut url = self.urls[document].join(&reference)?;
                        let pointer = url.fragment().unwrap_or_default().to_string();
                        url.set_fragment(None);
                        (self.load_url(url)?, pointer)
                    };
                    if !pointer.is_empty() && !pointer.starts_with('/') {
                        bail!("schema anchors are not supported: {reference}");
                    }
                    let target = if index == 0 { pointer.clone() } else { format!("/x-kestrel-documents/{index}{pointer}") };
                    object.insert("$ref".into(), json!(format!("#{target}")));
                    if seen.insert((index, pointer.clone())) {
                        let mut node = self.inputs[index].node(&pointer)?.with_context(|| format!("unresolved reference {reference}"))?;
                        self.expand(&mut node, index, context, seen, depth + 1)?;
                        insert(context, &target, node)?;
                    }
                }
                for child in object.values_mut() {
                    self.expand(child, document, context, seen, depth + 1)?;
                }
            }
            Json::Array(array) => {
                for child in array {
                    self.expand(child, document, context, seen, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn path(&mut self, path: &str) -> Result<Json> {
        self.inputs[0].node(&format!("/paths/{}", escape(path)))?.context("a specification path disappeared")
    }
}

fn insert(root: &mut Json, pointer: &str, value: Json) -> Result<()> {
    let mut parts = pointer.trim_start_matches('/').split('/').peekable();
    let mut current = root;
    while let Some(part) = parts.next() {
        let part = part.replace("~1", "/").replace("~0", "~");
        if parts.peek().is_none() {
            current[part] = value;
            return Ok(());
        }
        if current.get(&part).is_none() {
            current[&part] = json!({});
        }
        current = current.get_mut(&part).context("invalid reference path")?;
    }
    Ok(())
}
