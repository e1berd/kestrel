//! OpenAPI 3 into a collection, as often as the spec changes: a first import makes the collection,
//! and a sync later adds the new operations and refreshes the rest without losing what was typed
//! into them (bodies, parameter values, scripts, auth).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;
use volley_core::collection::{FOLDER_FILE, file_stem, sanitize_file_name};
use volley_core::format::{
    Auth, AuthConfig, Body, Docs, FolderFile, Http, Info, KeyValue, RequestFile, Settings,
};
use volley_core::{Collection, Document, Item};

const METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// A folder per operation's first tag.
    Tags,
    /// A folder per first path segment.
    Path,
}

impl GroupBy {
    pub fn parse(text: &str) -> Self {
        if text == "path" {
            GroupBy::Path
        } else {
            GroupBy::Tags
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GroupBy::Tags => "tags",
            GroupBy::Path => "path",
        }
    }
}

/// Reads a spec from a file (JSON or YAML) or an `http(s)://` URL. A relative path is taken from
/// `base`, the collection's folder.
pub fn load(source: &str, base: &Path) -> Result<Json> {
    let text = if source.starts_with("http://") || source.starts_with("https://") {
        let response =
            reqwest::blocking::get(source).with_context(|| format!("download {source}"))?;
        if !response.status().is_success() {
            bail!("{source} answered {}", response.status());
        }
        response.text()?
    } else {
        let path = if Path::new(source).is_absolute() {
            PathBuf::from(source)
        } else {
            base.join(source)
        };
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?
    };
    parse(&text)
}

pub fn parse(text: &str) -> Result<Json> {
    let spec: Json = match serde_json::from_str(text) {
        Ok(json) => json,
        Err(_) => serde_yaml_ng::from_str(text).context("the spec is neither JSON nor YAML")?,
    };
    match spec.get("openapi").and_then(Json::as_str) {
        Some(v) if v.starts_with('3') => Ok(spec),
        Some(v) => bail!("OpenAPI {v} is not supported; 3.0 and 3.1 are"),
        None if spec.get("swagger").is_some() => {
            bail!("Swagger 2.0 is not supported yet; convert it to OpenAPI 3 first")
        }
        None => bail!("that is not an OpenAPI spec (it has no `openapi` field)"),
    }
}

/// One operation, as a request file to be.
#[derive(Debug, Clone)]
pub struct Operation {
    /// The folder it goes in; `None` for the collection's root.
    pub folder: Option<String>,
    pub file: RequestFile,
    /// `METHOD /path/{}`: how a sync finds the request again, whatever it was renamed to.
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct Generated {
    pub title: String,
    pub base_url: Option<String>,
    pub operations: Vec<Operation>,
    /// Folder descriptions from the spec's `tags`.
    pub folder_docs: IndexMap<String, String>,
}

pub fn generate(spec: &Json, group_by: GroupBy) -> Generated {
    let title = spec
        .pointer("/info/title")
        .and_then(Json::as_str)
        .unwrap_or("API")
        .to_string();
    let base_url = spec
        .pointer("/servers/0/url")
        .and_then(Json::as_str)
        .map(str::to_string);
    let mut folder_docs = IndexMap::new();
    for tag in spec
        .get("tags")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(name), Some(description)) = (
            tag.get("name").and_then(Json::as_str),
            tag.get("description").and_then(Json::as_str),
        ) {
            folder_docs.insert(name.to_string(), description.to_string());
        }
    }
    let mut operations = Vec::new();
    let mut seq: BTreeMap<Option<String>, i64> = BTreeMap::new();
    let schemes = spec
        .pointer("/components/securitySchemes")
        .cloned()
        .unwrap_or(Json::Null);
    let global_security = spec.get("security").cloned();
    let empty = serde_json::Map::new();
    for (path, item) in spec
        .get("paths")
        .and_then(Json::as_object)
        .unwrap_or(&empty)
    {
        let item = resolve(spec, item);
        let shared_params: Vec<Json> = item
            .get("parameters")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        for method in METHODS {
            let Some(op) = item.get(method) else { continue };
            let tag = op
                .pointer("/tags/0")
                .and_then(Json::as_str)
                .map(str::to_string);
            let folder = match group_by {
                GroupBy::Tags => tag.clone(),
                GroupBy::Path => path
                    .split('/')
                    .find(|s| {
                        !s.is_empty()
                            && !s.starts_with('{')
                            && !matches!(*s, "api" | "v1" | "v2" | "v3")
                    })
                    .map(str::to_string),
            };
            let counter = seq.entry(folder.clone()).or_insert(0);
            *counter += 1;
            let mut params: Vec<Json> = shared_params.clone();
            for p in op
                .get("parameters")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
            {
                let p = resolve(spec, p);
                params.retain(|q| !(q.get("name") == p.get("name") && q.get("in") == p.get("in")));
                params.push(p.clone());
            }
            let security = op
                .get("security")
                .cloned()
                .or_else(|| global_security.clone());
            let file = request_file(
                spec,
                path,
                method,
                op,
                &params,
                security.as_ref(),
                &schemes,
                *counter,
            );
            operations.push(Operation {
                folder,
                file,
                key: key(method, path),
            });
        }
    }
    Generated {
        title,
        base_url,
        operations,
        folder_docs,
    }
}

fn key(method: &str, path: &str) -> String {
    let normalized: Vec<&str> = path
        .split('/')
        .map(|s| {
            if (s.starts_with('{') && s.ends_with('}')) || s.starts_with(':') {
                "{}"
            } else {
                s
            }
        })
        .collect();
    format!(
        "{} {}",
        method.to_uppercase(),
        normalized.join("/").trim_end_matches('/')
    )
}

/// The key of a request already in a collection: its method and its URL without `{{baseUrl}}`.
pub fn request_key(file: &RequestFile) -> Option<String> {
    let http = file.http.as_ref()?;
    let url = http.url.split(['?', '#']).next().unwrap_or_default();
    let path = match url.find("}}") {
        Some(i) if url.starts_with("{{") => url[i + 2..].to_string(),
        _ => url::Url::parse(url)
            .map(|u| u.path().to_string())
            .unwrap_or_else(|_| url.to_string()),
    };
    Some(key(&http.method, &path))
}

#[allow(clippy::too_many_arguments)]
fn request_file(
    spec: &Json,
    path: &str,
    method: &str,
    op: &Json,
    params: &[Json],
    security: Option<&Json>,
    schemes: &Json,
    seq: i64,
) -> RequestFile {
    let tags: Vec<String> = op
        .get("tags")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
        .map(str::to_string)
        .collect();
    let name = op
        .get("summary")
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| op.get("operationId").and_then(Json::as_str))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| format!("{} {path}", method.to_uppercase()));
    let url_path: String = path
        .split('/')
        .map(|s| {
            if s.starts_with('{') && s.ends_with('}') {
                format!(":{}", &s[1..s.len() - 1])
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let url = format!("{{{{baseUrl}}}}{url_path}");

    let mut kv_params = Vec::new();
    let mut headers = Vec::new();
    for p in params {
        let Some(name) = p.get("name").and_then(Json::as_str) else {
            continue;
        };
        let required = p.get("required").and_then(Json::as_bool).unwrap_or(false);
        let schema = p
            .get("schema")
            .map(|s| resolve(spec, s))
            .unwrap_or(Json::Null);
        let preset = p
            .get("example")
            .or_else(|| schema.get("example"))
            .or_else(|| schema.get("default"))
            .map(json_text);
        let description = p
            .get("description")
            .and_then(Json::as_str)
            .map(str::to_string);
        let mut kv = KeyValue::new(name, preset.clone().unwrap_or_default());
        kv.description = description;
        match p.get("in").and_then(Json::as_str) {
            Some("path") => {
                // A path parameter with a fixed set of values lists each, the first one on.
                let values: Vec<String> = schema
                    .get("enum")
                    .and_then(Json::as_array)
                    .map(|v| v.iter().map(json_text).collect())
                    .unwrap_or_default();
                if values.is_empty() {
                    kv_params.push(kv.with_kind("path"));
                }
                for (i, value) in values.iter().enumerate() {
                    let mut option = KeyValue::new(name, value.clone()).with_kind("path");
                    option.disabled = i > 0;
                    kv_params.push(option);
                }
            }
            Some("query") => {
                // A fixed set of values lists each, all off unless the parameter is required.
                let values: Vec<String> = schema
                    .get("enum")
                    .and_then(Json::as_array)
                    .map(|v| v.iter().map(json_text).collect())
                    .unwrap_or_default();
                if values.is_empty() {
                    kv.disabled = !required && preset.is_none();
                    kv_params.push(kv.with_kind("query"));
                }
                for (i, value) in values.iter().enumerate() {
                    let mut option = KeyValue::new(name, value.clone()).with_kind("query");
                    option.disabled = !required || i > 0;
                    kv_params.push(option);
                }
            }
            Some("header") => headers.push(kv),
            _ => {}
        }
    }
    // Path parameters in the order the path has them, as Bruno lists them.
    kv_params.sort_by_key(|p| {
        if p.kind.as_deref() == Some("path") {
            url_path.find(&format!(":{}", p.name)).unwrap_or(usize::MAX)
        } else {
            usize::MAX
        }
    });

    let body = op
        .get("requestBody")
        .map(|b| resolve(spec, b))
        .and_then(|body| request_body(spec, &body));
    let auth = security_auth(security, schemes);

    let mut extra = IndexMap::new();
    let examples = examples(spec, op, &url, method, &headers, &kv_params, body.as_ref());
    if !examples.is_empty() {
        extra.insert("examples".to_string(), Yaml::Sequence(examples));
    }
    let docs = op
        .get("description")
        .and_then(Json::as_str)
        .filter(|d| !d.trim().is_empty())
        .map(|d| Docs::Text(d.to_string()));
    RequestFile {
        info: Info {
            name,
            kind: "http".into(),
            seq: Some(seq),
            tags,
            ..Default::default()
        },
        http: Some(Http {
            method: method.to_uppercase(),
            url,
            params: kv_params,
            headers,
            body,
            auth: Some(auth),
            ..Default::default()
        }),
        settings: Some(Settings {
            extra: [("forwardAuthorizationHeader".to_string(), Yaml::Bool(false))]
                .into_iter()
                .collect(),
            ..Default::default()
        }),
        docs,
        extra,
        ..Default::default()
    }
}

fn request_body(spec: &Json, body: &Json) -> Option<Body> {
    let content = body.get("content")?.as_object()?;
    let (media, entry) = content
        .iter()
        .find(|(m, _)| m.contains("json"))
        .or_else(|| content.iter().next())?;
    let schema = entry.get("schema").cloned().unwrap_or(Json::Null);
    let example = entry.get("example").cloned().or_else(|| {
        entry
            .pointer("/examples")
            .and_then(|e| e.as_object()?.values().next()?.get("value").cloned())
    });
    let sample = example.unwrap_or_else(|| skeleton(spec, &schema, 0, &mut HashSet::new()));
    Some(if media.contains("json") {
        Body::text(
            "json",
            serde_json::to_string_pretty(&sample).unwrap_or_default(),
        )
    } else if media == "application/x-www-form-urlencoded" || media == "multipart/form-data" {
        let kind = if media.starts_with("multipart") {
            "multipart-form"
        } else {
            "form-urlencoded"
        };
        let mut body = Body {
            kind: kind.into(),
            ..Default::default()
        };
        let fields: Vec<KeyValue> = sample
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, v)| KeyValue::new(k, json_text(v)))
                    .collect()
            })
            .unwrap_or_default();
        body.set_fields(&fields);
        body
    } else if media.contains("xml") {
        Body::text("xml", "")
    } else {
        Body::text("text", json_text(&sample))
    })
}

fn security_auth(security: Option<&Json>, schemes: &Json) -> Auth {
    let Some(requirements) = security.and_then(Json::as_array) else {
        return Auth::Mode("inherit".into());
    };
    if requirements.is_empty() {
        return Auth::Mode("none".into());
    }
    for requirement in requirements {
        for name in requirement.as_object().into_iter().flat_map(|o| o.keys()) {
            let scheme = &schemes[name];
            match (
                scheme.get("type").and_then(Json::as_str),
                scheme
                    .get("scheme")
                    .and_then(Json::as_str)
                    .map(str::to_lowercase)
                    .as_deref(),
            ) {
                (Some("http"), Some("bearer")) | (Some("oauth2" | "openIdConnect"), _) => {
                    let mut config = AuthConfig::new("bearer");
                    config.set("token", "{{token}}");
                    return Auth::Config(config);
                }
                (Some("http"), Some("basic")) => {
                    let mut config = AuthConfig::new("basic");
                    config.set("username", "{{username}}");
                    config.set("password", "{{password}}");
                    return Auth::Config(config);
                }
                _ => {}
            }
        }
    }
    Auth::Mode("inherit".into())
}

fn examples(
    spec: &Json,
    op: &Json,
    url: &str,
    method: &str,
    headers: &[KeyValue],
    params: &[KeyValue],
    body: Option<&Body>,
) -> Vec<Yaml> {
    let mut out = Vec::new();
    for (status, response) in op
        .get("responses")
        .and_then(Json::as_object)
        .into_iter()
        .flatten()
    {
        let response = resolve(spec, response);
        let Ok(code) = status.parse::<u16>() else {
            continue;
        };
        let mut request = serde_yaml_ng::Mapping::new();
        request.insert("url".into(), url.into());
        request.insert("method".into(), method.to_uppercase().into());
        if !headers.is_empty() {
            request.insert(
                "headers".into(),
                serde_yaml_ng::to_value(headers).unwrap_or(Yaml::Null),
            );
        }
        if !params.is_empty() {
            request.insert(
                "params".into(),
                serde_yaml_ng::to_value(params).unwrap_or(Yaml::Null),
            );
        }
        if let Some(body) = body {
            request.insert(
                "body".into(),
                serde_yaml_ng::to_value(body).unwrap_or(Yaml::Null),
            );
        }
        let mut res = serde_yaml_ng::Mapping::new();
        res.insert("status".into(), Yaml::Number(code.into()));
        res.insert("statusText".into(), reason(code).into());
        let content = response.get("content").and_then(Json::as_object);
        match content.and_then(|c| c.iter().next()) {
            Some((media, entry)) => {
                let mut header = serde_yaml_ng::Mapping::new();
                header.insert("name".into(), "Content-Type".into());
                header.insert("value".into(), media.as_str().into());
                res.insert(
                    "headers".into(),
                    Yaml::Sequence(vec![Yaml::Mapping(header)]),
                );
                let sample = entry.get("example").cloned().unwrap_or_else(|| {
                    skeleton(
                        spec,
                        entry.get("schema").unwrap_or(&Json::Null),
                        0,
                        &mut HashSet::new(),
                    )
                });
                let mut body = serde_yaml_ng::Mapping::new();
                body.insert(
                    "type".into(),
                    if media.contains("json") {
                        "json"
                    } else {
                        "text"
                    }
                    .into(),
                );
                let data = match &sample {
                    Json::String(text) => text.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };
                body.insert("data".into(), data.into());
                res.insert("body".into(), Yaml::Mapping(body));
            }
            None => {
                let mut body = serde_yaml_ng::Mapping::new();
                body.insert("type".into(), "text".into());
                body.insert("data".into(), "".into());
                res.insert("body".into(), Yaml::Mapping(body));
            }
        }
        let mut example = serde_yaml_ng::Mapping::new();
        example.insert("name".into(), format!("{code} Response").into());
        if let Some(description) = response.get("description").and_then(Json::as_str) {
            example.insert("description".into(), description.into());
        }
        example.insert("request".into(), Yaml::Mapping(request));
        example.insert("response".into(), Yaml::Mapping(res));
        out.push(Yaml::Mapping(example));
    }
    out
}

/// `$ref` followed within the spec.
fn resolve(spec: &Json, value: &Json) -> Json {
    let mut current = value.clone();
    for _ in 0..16 {
        let Some(reference) = current.get("$ref").and_then(Json::as_str) else {
            return current;
        };
        let Some(pointer) = reference.strip_prefix('#') else {
            return current;
        };
        match spec.pointer(pointer) {
            Some(target) => current = target.clone(),
            None => return Json::Null,
        }
    }
    current
}

/// A sample value that has the schema's shape, with the keys in alphabetical order.
fn skeleton(spec: &Json, schema: &Json, depth: usize, seen: &mut HashSet<String>) -> Json {
    if depth > 10 {
        return Json::Null;
    }
    let reference = schema
        .get("$ref")
        .and_then(Json::as_str)
        .map(str::to_string);
    if let Some(r) = &reference {
        if !seen.insert(r.clone()) {
            return Json::Object(Default::default());
        }
    }
    let schema = resolve(spec, schema);
    let out = (|| {
        if let Some(example) = schema.get("example") {
            return example.clone();
        }
        if let Some(all) = schema.get("allOf").and_then(Json::as_array) {
            let mut merged = serde_json::Map::new();
            for part in all {
                if let Json::Object(map) = skeleton(spec, part, depth + 1, seen) {
                    merged.extend(map);
                }
            }
            return Json::Object(sorted(merged));
        }
        // Bruno samples a choice of types as an empty string; doing the same keeps a sync of its
        // collections from rewriting every example.
        if schema.get("oneOf").is_some() || schema.get("anyOf").is_some() {
            return Json::String(String::new());
        }
        if let Some(values) = schema.get("enum").and_then(Json::as_array) {
            return values.first().cloned().unwrap_or(Json::Null);
        }
        let kind = match schema.get("type") {
            Some(Json::String(t)) => t.clone(),
            Some(Json::Array(types)) => types
                .iter()
                .filter_map(Json::as_str)
                .find(|t| *t != "null")
                .unwrap_or("null")
                .to_string(),
            _ if schema.get("properties").is_some() => "object".into(),
            _ => String::new(),
        };
        match kind.as_str() {
            "object" => {
                let mut map = serde_json::Map::new();
                for (name, property) in schema
                    .get("properties")
                    .and_then(Json::as_object)
                    .into_iter()
                    .flatten()
                {
                    map.insert(name.clone(), skeleton(spec, property, depth + 1, seen));
                }
                Json::Object(sorted(map))
            }
            // One item, so the sample shows what goes in it.
            // One item when the items are objects, so the sample shows what goes in them.
            "array" => {
                let items = schema
                    .get("items")
                    .map(|items| resolve(spec, items))
                    .unwrap_or(Json::Null);
                match skeleton(
                    spec,
                    schema.get("items").unwrap_or(&Json::Null),
                    depth + 1,
                    seen,
                ) {
                    item @ Json::Object(_) => Json::Array(vec![item]),
                    item if items.get("enum").is_some() => Json::Array(vec![item]),
                    _ => Json::Array(Vec::new()),
                }
            }
            "integer" | "number" => Json::from(0),
            "boolean" => Json::Bool(false),
            "string" => Json::String(String::new()),
            "null" => Json::Null,
            _ => Json::String(String::new()),
        }
    })();
    if let Some(r) = reference {
        seen.remove(&r);
    }
    out
}

fn sorted(map: serde_json::Map<String, Json>) -> serde_json::Map<String, Json> {
    let mut entries: Vec<_> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().collect()
}

fn json_text(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

fn reason(code: u16) -> String {
    reqwest::StatusCode::from_u16(code)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("")
        .to_string()
}

/// What a sync did.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: usize,
    /// Requests the spec no longer has; deleted only when asked to prune.
    pub stale: Vec<String>,
    pub removed: Vec<String>,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} added, {} updated, {} unchanged",
            self.added.len(),
            self.updated.len(),
            self.unchanged
        )?;
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
    /// Delete requests whose operation left the spec.
    pub prune: bool,
    /// Report what would change without writing anything.
    pub dry_run: bool,
}

/// Brings `collection` in line with `spec`. What the spec owns (name, tags, docs, examples, the
/// parameters it lists) is refreshed; what people type (values, bodies, headers, auth, scripts,
/// settings) is kept.
pub fn sync(collection: &mut Collection, spec: &Json, options: &SyncOptions) -> Result<Report> {
    let generated = generate(spec, options.group_by);
    let mut report = Report::default();
    // Requests by key. Several may share one (a hand-made "Keycloak Login" on `GET /me`); an
    // operation takes the one with its name or its tags, never a request someone made by hand.
    let mut candidates: IndexMap<String, Vec<(PathBuf, String, Vec<String>)>> = IndexMap::new();
    for request in collection.requests() {
        if let Some(key) = request_key(&request.doc.data) {
            let info = &request.doc.data.info;
            candidates.entry(key).or_default().push((
                request.doc.path.clone(),
                info.name.clone(),
                info.tags.clone(),
            ));
        }
    }
    let mut existing: IndexMap<String, PathBuf> = IndexMap::new();
    for op in &generated.operations {
        let Some(list) = candidates.get(&op.key) else {
            continue;
        };
        let by_name = list.iter().find(|(_, name, _)| *name == op.file.info.name);
        let by_tags = list
            .iter()
            .find(|(_, _, tags)| !tags.is_empty() && *tags == op.file.info.tags);
        if let Some((path, _, _)) = by_name.or(by_tags) {
            existing.insert(op.key.clone(), path.clone());
        }
    }
    let generated_keys: HashSet<&String> = generated.operations.iter().map(|op| &op.key).collect();
    let mut stale_candidates: Vec<PathBuf> = Vec::new();
    for (key, list) in &candidates {
        for (path, _, tags) in list {
            let matched = existing.get(key) == Some(path);
            // Only requests an import made (they carry tags) count as gone from the spec.
            if !matched && !tags.is_empty() && !generated_keys.contains(key) {
                stale_candidates.push(path.clone());
            }
        }
    }
    let mut created_folders: IndexMap<String, PathBuf> = IndexMap::new();
    for op in &generated.operations {
        let label = format!("{} {}", op.file.http().method, op.file.info.name);
        if let Some(path) = existing.get(&op.key) {
            let request = collection
                .request_mut(path)
                .context("the request went away")?;
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
        let label = collection
            .request(path)
            .map(|r| format!("{} {}", r.method(), r.name()))
            .unwrap_or_else(|| path.display().to_string());
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

/// The spec's side of `file`, refreshed from `generated`.
fn refresh(file: &mut RequestFile, generated: &RequestFile) {
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
    // Parameters: the spec's list, with the values and switches already there kept.
    // A name may come several times (one per allowed value): the same value first, then any left.
    let old = std::mem::take(&mut http.params);
    let mut used = vec![false; old.len()];
    let mut take = |p: &KeyValue, same_value: bool| {
        let i = old.iter().enumerate().position(|(i, o)| {
            !used[i] && o.name == p.name && o.kind == p.kind && (!same_value || o.value == p.value)
        })?;
        used[i] = true;
        Some(old[i].clone())
    };
    let mut params = Vec::new();
    for p in &new_http.params {
        let kept = take(p, true).or_else(|| take(p, false));
        params.push(match kept {
            Some(o) => KeyValue {
                description: p.description.clone(),
                ..o
            },
            None => p.clone(),
        });
    }
    // Query parameters someone added with a value stay, though the spec does not know them.
    for (i, o) in old.iter().enumerate() {
        if !used[i]
            && o.kind.as_deref() == Some("query")
            && !o.value.is_empty()
            && !new_http.params.iter().any(|p| p.name == o.name)
        {
            params.push(o.clone());
        }
    }
    http.params = params;
    if http.body.is_none()
        || http
            .body
            .as_ref()
            .is_some_and(|b| b.as_text().trim().is_empty() && !b.is_form())
    {
        http.body = new_http.body.clone();
    }
    for header in &new_http.headers {
        if !http
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case(&header.name))
        {
            http.headers.push(header.clone());
        }
    }
}

fn folder_dir(collection: &Collection, name: &str, docs: Option<&String>) -> Result<PathBuf> {
    for item in &collection.items {
        if let Item::Folder(folder) = item {
            if folder.name() == name || file_stem(&folder.dir) == sanitize_file_name(name) {
                return Ok(folder.dir.clone());
            }
        }
    }
    let dir = collection.root.join(sanitize_file_name(name));
    std::fs::create_dir_all(&dir)?;
    let file = FolderFile {
        info: Info {
            name: name.into(),
            kind: "folder".into(),
            ..Default::default()
        },
        request: Some(volley_core::format::Defaults {
            auth: Some(Auth::Mode("inherit".into())),
            ..Default::default()
        }),
        docs: docs.map(|d| Docs::Rich {
            content: d.clone(),
            kind: Some("text/markdown".into()),
        }),
        ..Default::default()
    };
    Document::new(dir.join(FOLDER_FILE), file, true).save()?;
    Ok(dir)
}

fn unique_file(dir: &Path, name: &str) -> PathBuf {
    let stem = sanitize_file_name(name);
    (0..)
        .map(|n| {
            if n == 0 {
                dir.join(format!("{stem}.yml"))
            } else {
                dir.join(format!("{stem} {n}.yml"))
            }
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Makes a new collection in `dir` from `spec` and remembers where the spec is, so `sync` can run
/// without being told again.
pub fn import(
    dir: &Path,
    spec_source: &str,
    spec: &Json,
    group_by: GroupBy,
    name: Option<&str>,
) -> Result<(Collection, Report)> {
    let generated = generate(spec, group_by);
    let name = name
        .map(str::to_string)
        .unwrap_or_else(|| generated.title.clone());
    let mut collection = Collection::create(dir, &name)?;
    let mut ext = serde_yaml_ng::Mapping::new();
    let mut source = serde_yaml_ng::Mapping::new();
    source.insert("sourceUrl".into(), spec_source.into());
    source.insert("groupBy".into(), group_by.as_str().into());
    ext.insert(
        "openapi".into(),
        Yaml::Sequence(vec![Yaml::Mapping(source)]),
    );
    let mut extensions = serde_yaml_ng::Mapping::new();
    extensions.insert("volley".into(), Yaml::Mapping(ext));
    collection
        .doc
        .data
        .extra
        .insert("extensions".into(), Yaml::Mapping(extensions));
    if let Some(description) = spec.pointer("/info/description").and_then(Json::as_str) {
        collection.doc.data.docs = Some(Docs::Rich {
            content: description.into(),
            kind: Some("text/markdown".into()),
        });
    }
    let base_url = generated
        .base_url
        .clone()
        .unwrap_or_else(|| "http://localhost:8000".into());
    collection.doc.data.request = Some(volley_core::format::Defaults {
        variables: vec![KeyValue::new("baseUrl", base_url)],
        ..Default::default()
    });
    collection.doc.save()?;
    let report = sync(
        &mut collection,
        spec,
        &SyncOptions {
            group_by,
            prune: false,
            dry_run: false,
        },
    )?;
    Ok((collection, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r##"{
      "openapi": "3.1.0",
      "info": {"title": "Pets"},
      "components": {
        "securitySchemes": {"B": {"type": "http", "scheme": "bearer"}},
        "schemas": {"Pet": {"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}, "tags": {"type": "array", "items": {"type": "string"}}}}}
      },
      "security": [{"B": []}],
      "paths": {
        "/pets/{pet_id}": {
          "get": {"summary": "Get Pet", "tags": ["pets"], "parameters": [{"in": "path", "name": "pet_id", "required": true}, {"in": "query", "name": "full", "schema": {"type": "boolean", "default": false}}, {"in": "query", "name": "q"}],
                  "responses": {"200": {"description": "OK", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}}}},
          "patch": {"summary": "Update Pet: partly", "tags": ["pets"], "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}}, "responses": {"204": {"description": "Done"}}}
        }
      }
    }"##;

    #[test]
    fn operations_become_requests() {
        let spec = parse(SPEC).unwrap();
        let generated = generate(&spec, GroupBy::Tags);
        assert_eq!(generated.operations.len(), 2);
        let get = &generated.operations[0];
        assert_eq!(get.folder.as_deref(), Some("pets"));
        assert_eq!(get.key, "GET /pets/{}");
        let http = get.file.http();
        assert_eq!(http.url, "{{baseUrl}}/pets/:pet_id");
        assert_eq!(
            http.params
                .iter()
                .map(|p| (p.name.as_str(), p.value.as_str(), p.disabled))
                .collect::<Vec<_>>(),
            vec![
                ("pet_id", "", false),
                ("full", "false", false),
                ("q", "", true)
            ]
        );
        assert_eq!(http.auth.unwrap().mode(), "bearer");
        let patch = generated.operations[1].file.http();
        assert_eq!(
            patch.body.unwrap().as_text(),
            "{\n  \"age\": 0,\n  \"name\": \"\",\n  \"tags\": []\n}"
        );
        assert_eq!(
            request_key(&generated.operations[1].file).as_deref(),
            Some("PATCH /pets/{}")
        );
    }

    #[test]
    fn import_then_sync_keeps_edits() {
        let dir = std::env::temp_dir().join(format!("kestrel-openapi-{}", uuid::Uuid::new_v4()));
        let spec = parse(SPEC).unwrap();
        let (mut collection, report) =
            import(&dir, "spec.json", &spec, GroupBy::Tags, None).unwrap();
        assert_eq!(report.added.len(), 2);
        let path = collection
            .requests()
            .iter()
            .find(|r| r.method() == "PATCH")
            .unwrap()
            .doc
            .path
            .clone();
        assert!(path.ends_with("pets/Update Pet- partly.yml"));
        {
            let request = collection.request_mut(&path).unwrap();
            request.doc.data.http.as_mut().unwrap().body =
                Some(Body::text("json", "{\"name\": \"Rex\"}"));
            request.doc.save().unwrap();
        }
        let report = sync(
            &mut collection,
            &spec,
            &SyncOptions {
                group_by: GroupBy::Tags,
                prune: false,
                dry_run: false,
            },
        )
        .unwrap();
        assert_eq!(
            (report.added.len(), report.updated.len(), report.unchanged),
            (0, 0, 2)
        );
        let body = collection
            .request(&path)
            .unwrap()
            .doc
            .data
            .http()
            .body
            .unwrap()
            .as_text();
        assert_eq!(body, "{\"name\": \"Rex\"}");
        assert_eq!(
            collection.doc.data.openapi_sources(),
            vec![("spec.json".to_string(), "tags".to_string())]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
