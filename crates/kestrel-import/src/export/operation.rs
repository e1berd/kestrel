use anyhow::{Context as _, Result};
use kestrel_core::format::{Auth, Body, KeyValue, RequestFile};
use serde_json::{Value as Json, json};

use super::ExportFormat;
use super::index::{Entry, auth};

pub(super) fn operation(request: &RequestFile, entry: &Entry, format: ExportFormat) -> Result<Json> {
    let http = request.http.as_ref().context("export an HTTP request")?;
    let mut parameters = Vec::new();
    for parameter in &http.params {
        parameters.push(parameter_json(parameter, parameter.kind.as_deref().unwrap_or("query")));
    }
    for header in entry.defaults.headers.iter().chain(&http.headers) {
        parameters.push(parameter_json(header, "header"));
    }
    if let Some((_, query)) = http.url.split_once('?') {
        for (name, value) in url::form_urlencoded::parse(query.split('#').next().unwrap_or("").as_bytes()) {
            parameters.push(parameter_json(&KeyValue::new(name, value), "query"));
        }
    }
    let effective_auth = auth(request, entry);
    if let Auth::Config(config) = &effective_auth
        && matches!(config.kind.as_str(), "apikey" | "api-key")
    {
        let parameter = KeyValue::new(config.str("key"), "{{process.env.API_KEY}}");
        parameters.push(parameter_json(&parameter, if config.str("placement") == "query" { "query" } else { "header" }));
    }
    let mut operation = json!({"summary": request.info.name, "parameters": parameters, "responses": responses(request)?});
    if let Some(docs) = &request.docs {
        operation["description"] = json!(docs.content());
    }
    let tags = if request.info.tags.is_empty() {
        entry.folders.last().cloned().into_iter().collect::<Vec<_>>()
    } else {
        request.info.tags.clone()
    };
    if !tags.is_empty() {
        operation["tags"] = json!(tags);
    }
    operation["security"] = match effective_auth.mode() {
        "basic" => json!([{"basic": []}]),
        "bearer" | "oauth2" => json!([{"bearer": []}]),
        _ => json!([]),
    };
    if http.url.contains("://") {
        let start = http.url.find("://").expect("an absolute URL") + 3;
        let end = http.url[start..].find('/').map(|offset| offset + start).unwrap_or(http.url.len());
        operation["servers"] = json!([{"url": &http.url[..end]}]);
    }
    if let Some(body) = &http.body {
        operation["requestBody"] = body_json(body)?;
    }
    if format == ExportFormat::Swagger20 {
        swagger(&mut operation);
    }
    Ok(operation)
}

fn parameter_json(parameter: &KeyValue, location: &str) -> Json {
    let value = if parameter.name.eq_ignore_ascii_case("authorization") || parameter.name.eq_ignore_ascii_case("proxy-authorization") {
        "{{process.env.AUTHORIZATION}}"
    } else if parameter.name.eq_ignore_ascii_case("x-api-key") {
        "{{process.env.API_KEY}}"
    } else {
        &parameter.value
    };
    let mut result = json!({"name": parameter.name, "in": location, "required": location == "path", "example": value,
        "schema": {"type": "string"}, "x-kestrel-disabled": parameter.disabled});
    if let Some(description) = &parameter.description {
        result["description"] = json!(description);
    }
    result
}

fn body_json(body: &Body) -> Result<Json> {
    let media = match body.kind.as_str() {
        "json" | "graphql" => "application/json",
        "xml" => "application/xml",
        "form-urlencoded" => "application/x-www-form-urlencoded",
        "multipart-form" => "multipart/form-data",
        _ => "text/plain",
    };
    let entry = if body.is_form() {
        let mut properties = serde_json::Map::new();
        let mut example = serde_json::Map::new();
        for field in body.fields() {
            let mut schema = json!({"type": "string", "x-kestrel-disabled": field.disabled});
            if field.kind.as_deref() == Some("file") {
                schema["format"] = json!("binary");
            }
            schema["example"] = json!(field.value);
            properties.insert(field.name.clone(), schema);
            example.insert(field.name, json!(field.value));
        }
        json!({"schema": {"type": "object", "properties": properties}, "example": example})
    } else if body.kind == "graphql" {
        let (query, variables) = body.graphql();
        let variables: Json = if variables.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&variables).context("export GraphQL variables as JSON")?
        };
        json!({"example": {"query": query, "variables": variables}})
    } else {
        let text = body.as_text();
        let example = if body.kind == "json" && !text.trim().is_empty() {
            serde_json::from_str::<Json>(&text).context("export a JSON body; it must contain valid JSON")?
        } else {
            json!(text)
        };
        json!({"example": example})
    };
    Ok(json!({"content": {media: entry}}))
}

fn responses(request: &RequestFile) -> Result<Json> {
    let mut responses = serde_json::Map::new();
    for example in request.extra.get("examples").and_then(serde_yaml_ng::Value::as_sequence).into_iter().flatten() {
        let response = &example["response"];
        let Some(status) = response["status"].as_u64() else { continue };
        let body = &response["body"];
        let text = body["data"].as_str().unwrap_or("");
        let media = if body["type"].as_str() == Some("json") { "application/json" } else { "text/plain" };
        let data = if media.contains("json") && !text.is_empty() {
            match serde_json::from_str::<Json>(text) {
                Ok(value) => value,
                Err(_) => json!(text),
            }
        } else {
            json!(text)
        };
        responses.insert(
            status.to_string(),
            json!({"description": example["description"].as_str().unwrap_or("Response"), "content": {media: {"example": data}}}),
        );
    }
    if responses.is_empty() {
        responses.insert("200".into(), json!({"description": "Response"}));
    }
    Ok(responses.into())
}

fn swagger(operation: &mut Json) {
    if let Some(parameters) = operation.get_mut("parameters").and_then(Json::as_array_mut) {
        for parameter in parameters {
            parameter["type"] = json!("string");
            parameter["default"] = parameter["example"].clone();
            parameter.as_object_mut().expect("a parameter").remove("schema");
        }
    }
    if let Some(body) = operation.get("requestBody").cloned()
        && let Some(content) = body["content"].as_object()
    {
        let mut consumes = Vec::new();
        for (media, entry) in content {
            consumes.push(media.clone());
            if media.contains("form") {
                for (name, property) in entry["schema"]["properties"].as_object().into_iter().flatten() {
                    let kind = if property["format"] == "binary" { "file" } else { "string" };
                    operation["parameters"]
                        .as_array_mut()
                        .expect("request parameters")
                        .push(json!({"name": name, "in": "formData", "type": kind, "example": property["example"], "x-kestrel-disabled": property["x-kestrel-disabled"]}));
                }
            } else {
                let mut schema = entry.get("schema").cloned().unwrap_or(json!({}));
                schema["example"] = entry["example"].clone();
                operation["parameters"]
                    .as_array_mut()
                    .expect("request parameters")
                    .push(json!({"name": "body", "in": "body", "schema": schema}));
            }
        }
        operation["consumes"] = json!(consumes);
        operation.as_object_mut().expect("an operation").remove("requestBody");
    }
    for response in operation["responses"].as_object_mut().into_iter().flat_map(|responses| responses.values_mut()) {
        if let Some(content) = response.as_object_mut().expect("a response").remove("content") {
            let examples: serde_json::Map<_, _> =
                content.as_object().into_iter().flatten().map(|(media, entry)| (media.clone(), entry["example"].clone())).collect();
            response["examples"] = examples.into();
        }
    }
    operation.as_object_mut().expect("an operation").remove("servers");
}
