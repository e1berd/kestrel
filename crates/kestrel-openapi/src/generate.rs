use std::collections::{BTreeMap, HashSet};

use crate::samples::{examples, json_text, media_example, resolve, skeleton};
use crate::{Generated, GroupBy, METHODS, Operation};
use indexmap::IndexMap;
use kestrel_core::format::{Auth, AuthConfig, Body, Docs, Http, Info, KeyValue, RequestFile, Settings};
use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;

pub fn generate(spec: &Json, group_by: GroupBy) -> Generated {
    let title = spec.pointer("/info/title").and_then(Json::as_str).unwrap_or("API").to_string();
    let base_url = spec.pointer("/servers/0").and_then(server_url);
    let mut folder_docs = IndexMap::new();
    for tag in spec.get("tags").and_then(Json::as_array).into_iter().flatten() {
        if let (Some(name), Some(description)) = (tag.get("name").and_then(Json::as_str), tag.get("description").and_then(Json::as_str)) {
            folder_docs.insert(name.to_string(), description.to_string());
        }
    }
    let mut operations = Vec::new();
    let mut seq: BTreeMap<Option<String>, i64> = BTreeMap::new();
    let schemes = spec.pointer("/components/securitySchemes").cloned().unwrap_or(Json::Null);
    let global_security = spec.get("security").cloned();
    let empty = serde_json::Map::new();
    for (path, item) in spec.get("paths").and_then(Json::as_object).unwrap_or(&empty) {
        let item = resolve(spec, item);
        let shared_params: Vec<Json> =
            item.get("parameters").and_then(Json::as_array).into_iter().flatten().map(|parameter| resolve(spec, parameter)).collect();
        let path_operations = METHODS.into_iter().filter_map(|method| item.get(method).map(|operation| (method, operation))).chain(
            item.get("additionalOperations")
                .and_then(Json::as_object)
                .into_iter()
                .flatten()
                .map(|(method, operation)| (method.as_str(), operation)),
        );
        for (method, op) in path_operations {
            let tag = op.pointer("/tags/0").and_then(Json::as_str).map(str::to_string);
            let folder = match group_by {
                GroupBy::Tags => tag.clone(),
                GroupBy::Path => path
                    .split('/')
                    .find(|s| !s.is_empty() && !s.starts_with('{') && !matches!(*s, "api" | "v1" | "v2" | "v3"))
                    .map(str::to_string),
            };
            let counter = seq.entry(folder.clone()).or_insert(0);
            *counter += 1;
            let mut params: Vec<Json> = shared_params.clone();
            for p in op.get("parameters").and_then(Json::as_array).into_iter().flatten() {
                let p = resolve(spec, p);
                params.retain(|q| !(q.get("name") == p.get("name") && q.get("in") == p.get("in")));
                params.push(p.clone());
            }
            let security = op.get("security").cloned().or_else(|| global_security.clone());
            let mut file = request_file(spec, path, method, op, &params, security.as_ref(), &schemes, *counter);
            if let Some(server) = op.pointer("/servers/0").or_else(|| item.pointer("/servers/0")).and_then(server_url)
                && let Some(http) = &mut file.http
            {
                http.url = http.url.replacen("{{baseUrl}}", server.trim_end_matches('/'), 1);
            }
            operations.push(Operation { folder, file, key: key(method, path) });
        }
    }
    Generated { title, base_url, operations, folder_docs }
}

fn server_url(server: &Json) -> Option<String> {
    let mut url = server.get("url")?.as_str()?.to_string();
    for (name, variable) in server.get("variables").and_then(Json::as_object).into_iter().flatten() {
        if let Some(value) = variable.get("default") {
            url = url.replace(&format!("{{{name}}}"), &json_text(value));
        }
    }
    Some(url)
}
pub(crate) fn key(method: &str, path: &str) -> String {
    let normalized: Vec<&str> =
        path.split('/').map(|s| if (s.starts_with('{') && s.ends_with('}')) || s.starts_with(':') { "{}" } else { s }).collect();
    format!("{} {}", method.to_uppercase(), normalized.join("/").trim_end_matches('/'))
}
pub fn request_key(file: &RequestFile) -> Option<String> {
    let http = file.http.as_ref()?;
    let url = http.url.split(['?', '#']).next().unwrap_or_default();
    let path = match url.find("}}") {
        Some(i) if url.starts_with("{{") => url[i + 2..].to_string(),
        _ => url::Url::parse(url).map(|u| u.path().to_string()).unwrap_or_else(|_| url.to_string()),
    };
    Some(key(&http.method, &path))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn request_file(
    spec: &Json,
    path: &str,
    method: &str,
    op: &Json,
    params: &[Json],
    security: Option<&Json>,
    schemes: &Json,
    seq: i64,
) -> RequestFile {
    let tags: Vec<String> =
        op.get("tags").and_then(Json::as_array).into_iter().flatten().filter_map(Json::as_str).map(str::to_string).collect();
    let name = op
        .get("summary")
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| op.get("operationId").and_then(Json::as_str))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| format!("{} {path}", method.to_uppercase()));
    let url_path: String = path
        .split('/')
        .map(|s| if s.starts_with('{') && s.ends_with('}') { format!(":{}", &s[1..s.len() - 1]) } else { s.to_string() })
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
        let schema = p.get("schema").map(|s| resolve(spec, s)).unwrap_or(Json::Null);
        let preset = p.get("example").or_else(|| schema.get("example")).or_else(|| schema.get("default")).map(json_text);
        let description = p.get("description").and_then(Json::as_str).map(str::to_string);
        let mut kv = KeyValue::new(name, preset.clone().unwrap_or_default());
        kv.description = description;
        match p.get("in").and_then(Json::as_str) {
            Some("path") => {
                let values: Vec<String> =
                    schema.get("enum").and_then(Json::as_array).map(|v| v.iter().map(json_text).collect()).unwrap_or_default();
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
                let values: Vec<String> =
                    schema.get("enum").and_then(Json::as_array).map(|v| v.iter().map(json_text).collect()).unwrap_or_default();
                if values.is_empty() {
                    kv.disabled = p.get("x-kestrel-disabled").and_then(Json::as_bool).unwrap_or(!required && preset.is_none());
                    kv_params.push(kv.with_kind("query"));
                }
                for (i, value) in values.iter().enumerate() {
                    let mut option = KeyValue::new(name, value.clone()).with_kind("query");
                    option.disabled = !required || i > 0;
                    kv_params.push(option);
                }
            }
            Some("header") => {
                kv.disabled = p.get("x-kestrel-disabled").and_then(Json::as_bool).unwrap_or(false);
                headers.push(kv);
            }
            _ => {}
        }
    }
    kv_params.sort_by_key(|p| {
        if p.kind.as_deref() == Some("path") { url_path.find(&format!(":{}", p.name)).unwrap_or(usize::MAX) } else { usize::MAX }
    });

    let body = op.get("requestBody").map(|b| resolve(spec, b)).and_then(|body| request_body(spec, &body));
    let auth = security_auth(security, schemes);

    let mut extra = IndexMap::new();
    let examples = examples(spec, op, &url, method, &headers, &kv_params, body.as_ref());
    if !examples.is_empty() {
        extra.insert("examples".to_string(), Yaml::Sequence(examples));
    }
    let docs = op.get("description").and_then(Json::as_str).filter(|d| !d.trim().is_empty()).map(|d| Docs::Text(d.to_string()));
    RequestFile {
        info: Info { name, kind: "http".into(), seq: Some(seq), tags, ..Default::default() },
        http: Some(Http { method: method.to_uppercase(), url, params: kv_params, headers, body, auth: Some(auth), ..Default::default() }),
        settings: Some(Settings {
            extra: [("forwardAuthorizationHeader".to_string(), Yaml::Bool(false))].into_iter().collect(),
            ..Default::default()
        }),
        docs,
        extra,
        ..Default::default()
    }
}
pub(crate) fn request_body(spec: &Json, body: &Json) -> Option<Body> {
    let content = body.get("content")?.as_object()?;
    let (media, entry) = content.iter().find(|(m, _)| m.contains("json")).or_else(|| content.iter().next())?;
    let schema = entry.get("schema").cloned().unwrap_or(Json::Null);
    let example = media_example(spec, entry);
    let sample = example.unwrap_or_else(|| skeleton(spec, &schema, 0, &mut HashSet::new()));
    Some(if media.contains("json") {
        Body::text("json", serde_json::to_string_pretty(&sample).unwrap_or_default())
    } else if media == "application/x-www-form-urlencoded" || media == "multipart/form-data" {
        let kind = if media.starts_with("multipart") { "multipart-form" } else { "form-urlencoded" };
        let mut body = Body { kind: kind.into(), ..Default::default() };
        let schema = resolve(spec, &schema);
        let fields: Vec<KeyValue> = sample
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .map(|(name, value)| {
                        let mut field = KeyValue::new(name, json_text(value));
                        let property = schema
                            .get("properties")
                            .and_then(|properties| properties.get(name))
                            .map(|property| resolve(spec, property))
                            .unwrap_or(Json::Null);
                        if property["format"] == "binary" || property["type"] == "file" {
                            field.kind = Some("file".into());
                        }
                        field.disabled = property.get("x-kestrel-disabled").and_then(Json::as_bool).unwrap_or(false);
                        field.description = property.get("description").and_then(Json::as_str).map(str::to_string);
                        field
                    })
                    .collect()
            })
            .unwrap_or_default();
        body.set_fields(&fields);
        body
    } else if media.contains("xml") {
        Body::text("xml", json_text(&sample))
    } else {
        Body::text("text", json_text(&sample))
    })
}
pub(crate) fn security_auth(security: Option<&Json>, schemes: &Json) -> Auth {
    let Some(requirements) = security.and_then(Json::as_array) else {
        return Auth::Mode("inherit".into());
    };
    if requirements.is_empty() {
        return Auth::Mode("none".into());
    }
    for requirement in requirements {
        for name in requirement.as_object().into_iter().flat_map(|o| o.keys()) {
            let scheme = &schemes[name];
            match (scheme.get("type").and_then(Json::as_str), scheme.get("scheme").and_then(Json::as_str).map(str::to_lowercase).as_deref())
            {
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
                (Some("apiKey"), _) => {
                    let mut config = AuthConfig::new("apikey");
                    config.set("key", scheme.get("name").and_then(Json::as_str).unwrap_or("X-API-Key"));
                    config.set("value", "{{process.env.API_KEY}}");
                    config.set("placement", scheme.get("in").and_then(Json::as_str).unwrap_or("header"));
                    return Auth::Config(config);
                }
                _ => {}
            }
        }
    }
    Auth::Mode("inherit".into())
}
