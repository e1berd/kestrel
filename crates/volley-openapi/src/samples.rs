use std::collections::HashSet;

use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;
use volley_core::format::{Body, KeyValue};

pub(crate) fn examples(spec: &Json, op: &Json, url: &str, method: &str, headers: &[KeyValue], params: &[KeyValue], body: Option<&Body>) -> Vec<Yaml> {
    let mut out = Vec::new();
    for (status, response) in op.get("responses").and_then(Json::as_object).into_iter().flatten() {
        let response = resolve(spec, response);
        let Ok(code) = status.parse::<u16>() else {
            continue;
        };
        let mut request = serde_yaml_ng::Mapping::new();
        request.insert("url".into(), url.into());
        request.insert("method".into(), method.to_uppercase().into());
        if !headers.is_empty() {
            request.insert("headers".into(), serde_yaml_ng::to_value(headers).unwrap_or(Yaml::Null));
        }
        if !params.is_empty() {
            request.insert("params".into(), serde_yaml_ng::to_value(params).unwrap_or(Yaml::Null));
        }
        if let Some(body) = body {
            request.insert("body".into(), serde_yaml_ng::to_value(body).unwrap_or(Yaml::Null));
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
                res.insert("headers".into(), Yaml::Sequence(vec![Yaml::Mapping(header)]));
                let sample = entry
                    .get("example")
                    .cloned()
                    .unwrap_or_else(|| skeleton(spec, entry.get("schema").unwrap_or(&Json::Null), 0, &mut HashSet::new()));
                let mut body = serde_yaml_ng::Mapping::new();
                body.insert("type".into(), if media.contains("json") { "json" } else { "text" }.into());
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
pub(crate) fn resolve(spec: &Json, value: &Json) -> Json {
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
pub(crate) fn skeleton(spec: &Json, schema: &Json, depth: usize, seen: &mut HashSet<String>) -> Json {
    if depth > 10 {
        return Json::Null;
    }
    let reference = schema.get("$ref").and_then(Json::as_str).map(str::to_string);
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
        if schema.get("oneOf").is_some() || schema.get("anyOf").is_some() {
            return Json::String(String::new());
        }
        if let Some(values) = schema.get("enum").and_then(Json::as_array) {
            return values.first().cloned().unwrap_or(Json::Null);
        }
        let kind = match schema.get("type") {
            Some(Json::String(t)) => t.clone(),
            Some(Json::Array(types)) => types.iter().filter_map(Json::as_str).find(|t| *t != "null").unwrap_or("null").to_string(),
            _ if schema.get("properties").is_some() => "object".into(),
            _ => String::new(),
        };
        match kind.as_str() {
            "object" => {
                let mut map = serde_json::Map::new();
                for (name, property) in schema.get("properties").and_then(Json::as_object).into_iter().flatten() {
                    map.insert(name.clone(), skeleton(spec, property, depth + 1, seen));
                }
                Json::Object(sorted(map))
            }
            "array" => {
                let items = schema.get("items").map(|items| resolve(spec, items)).unwrap_or(Json::Null);
                match skeleton(spec, schema.get("items").unwrap_or(&Json::Null), depth + 1, seen) {
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
pub(crate) fn sorted(map: serde_json::Map<String, Json>) -> serde_json::Map<String, Json> {
    let mut entries: Vec<_> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().collect()
}
pub(crate) fn json_text(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}
pub(crate) fn reason(code: u16) -> String {
    reqwest::StatusCode::from_u16(code).ok().and_then(|s| s.canonical_reason()).unwrap_or("").to_string()
}
