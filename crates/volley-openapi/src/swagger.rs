use anyhow::{Result, bail};
use serde_json::{Value as Json, json};

use crate::METHODS;
use crate::samples::resolve;

pub(super) fn convert(mut spec: Json) -> Result<Json> {
    if !spec.get("info").is_some_and(Json::is_object) || !spec.get("paths").is_some_and(Json::is_object) {
        bail!("Swagger 2.0 must contain `info` and `paths` objects");
    }
    let original = spec.clone();
    let media = original.get("produces").cloned().unwrap_or(json!(["application/json"]));
    let consumes = original.get("consumes").cloned().unwrap_or(json!(["application/json"]));
    let host = spec.get("host").and_then(Json::as_str).unwrap_or("localhost");
    let scheme = spec.pointer("/schemes/0").and_then(Json::as_str).unwrap_or("https");
    let base = spec.get("basePath").and_then(Json::as_str).unwrap_or("");
    spec["servers"] = json!([{"url": format!("{scheme}://{host}{base}")}]);
    spec["openapi"] = json!("3.0.3");
    let mut schemes = original.get("securityDefinitions").cloned().unwrap_or(json!({}));
    for scheme in schemes.as_object_mut().into_iter().flat_map(|schemes| schemes.values_mut()) {
        if scheme["type"] == "basic" {
            *scheme = json!({"type": "http", "scheme": "basic"});
        }
    }
    spec["components"] = json!({"securitySchemes": schemes});
    for item in spec["paths"].as_object_mut().into_iter().flat_map(|paths| paths.values_mut()) {
        *item = resolve(&original, item);
        if !item.is_object() {
            bail!("Swagger path items must be objects");
        }
        let shared: Vec<_> =
            item.get("parameters").and_then(Json::as_array).into_iter().flatten().map(|parameter| resolve(&original, parameter)).collect();
        for method in METHODS {
            let Some(operation) = item.get_mut(method) else { continue };
            if !operation.is_object() {
                bail!("Swagger operations must be objects");
            }
            let mut parameters = shared.clone();
            for parameter in operation.get("parameters").and_then(Json::as_array).into_iter().flatten() {
                let parameter = resolve(&original, parameter);
                parameters.retain(|shared| shared["name"] != parameter["name"] || shared["in"] != parameter["in"]);
                parameters.push(parameter);
            }
            let parameters: Vec<_> = parameters.iter().map(|parameter| resolve(&original, parameter)).collect();
            let consumes = operation.get("consumes").unwrap_or(&consumes);
            if let Some(body) = request_body(&parameters, consumes) {
                operation["requestBody"] = body;
            }
            operation["parameters"] = Json::Array(parameters.into_iter().filter_map(parameter).collect());
            let produces = operation.get("produces").unwrap_or(&media).clone();
            for response in operation["responses"].as_object_mut().into_iter().flat_map(|responses| responses.values_mut()) {
                *response = resolve(&original, response);
                if response.get("schema").is_some() || response.get("examples").is_some() {
                    let mut content = serde_json::Map::new();
                    for media in produces.as_array().into_iter().flatten().filter_map(Json::as_str) {
                        let mut entry = json!({});
                        if let Some(schema) = response.get("schema") {
                            entry["schema"] = schema.clone();
                        }
                        if let Some(example) = response.get("examples").and_then(|examples| examples.get(media)) {
                            entry["example"] = example.clone();
                        }
                        content.insert(media.into(), entry);
                    }
                    response["content"] = content.into();
                }
            }
        }
        if let Some(item) = item.as_object_mut() {
            item.remove("parameters");
        }
    }
    Ok(spec)
}

fn parameter(mut parameter: Json) -> Option<Json> {
    match parameter["in"].as_str()? {
        "body" | "formData" => None,
        _ => {
            if parameter.get("schema").is_none() {
                let mut schema = serde_json::Map::new();
                for key in ["type", "format", "items", "enum", "default", "example", "x-kestrel-disabled", "description"] {
                    if let Some(value) = parameter.get(key) {
                        schema.insert(key.into(), value.clone());
                    }
                }
                parameter["schema"] = schema.into();
            }
            Some(parameter)
        }
    }
}

fn request_body(parameters: &[Json], consumes: &Json) -> Option<Json> {
    let body = parameters.iter().find(|parameter| parameter["in"] == "body");
    let fields: Vec<_> = parameters.iter().filter(|parameter| parameter["in"] == "formData").collect();
    if body.is_none() && fields.is_empty() {
        return None;
    }
    let schema = match body {
        Some(body) => body.get("schema").cloned().unwrap_or(json!({})),
        None => {
            let properties: serde_json::Map<_, _> = fields
                .iter()
                .filter_map(|field| {
                    let name = field["name"].as_str()?;
                    let mut field = (*field).clone();
                    field["in"] = json!("query");
                    let schema = parameter(field)?;
                    Some((name.into(), schema["schema"].clone()))
                })
                .collect();
            json!({"type": "object", "properties": properties})
        }
    };
    let content: serde_json::Map<_, _> =
        consumes.as_array().into_iter().flatten().filter_map(Json::as_str).map(|media| (media.into(), json!({"schema": schema}))).collect();
    Some(json!({"content": content}))
}
