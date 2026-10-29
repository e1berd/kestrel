use anyhow::{Context as _, Result};
use serde_json::{Value as Json, json};
use volley_core::format::{KeyValue, RequestFile};

use super::index::{Entry, auth};

pub(super) fn request(request: &RequestFile, entry: &Entry) -> Result<Json> {
    let http = request.http.as_ref().context("export an HTTP request")?;
    let mut parameters = json!({"path": [], "query": [], "header": [], "cookie": []});
    for parameter in &http.params {
        let location = parameter.kind.as_deref().filter(|kind| *kind == "path").unwrap_or("query");
        parameters[location].as_array_mut().expect("a parameter list").push(parameter_json(parameter));
    }
    for header in entry.defaults.headers.iter().chain(&http.headers) {
        parameters["header"].as_array_mut().expect("header list").push(parameter_json(header));
    }
    let mut api =
        json!({"method": http.method.to_ascii_lowercase(), "path": http.url, "parameters": parameters, "tags": request.info.tags});
    if let Some(docs) = &request.docs {
        api["description"] = json!(docs.content());
    }
    let effective_auth = auth(request, entry);
    api["auth"] = match effective_auth.mode() {
        "bearer" | "oauth2" => json!({"type": "bearer", "bearer": {"token": "{{process.env.API_TOKEN}}"}}),
        "basic" => {
            json!({"type": "basic", "basic": {"username": "{{process.env.API_USERNAME}}", "password": "{{process.env.API_PASSWORD}}"}})
        }
        "apikey" | "api-key" => {
            let config = effective_auth.config().context("API key configuration")?;
            json!({"type": "apikey", "apikey": {"key": config.str("key"), "in": config.str("placement"), "value": "{{process.env.API_KEY}}"}})
        }
        _ => json!({"type": "noauth"}),
    };
    if let Some(body) = &http.body {
        let media = match body.kind.as_str() {
            "json" => "application/json",
            "xml" => "application/xml",
            "graphql" => "graphql",
            "form-urlencoded" => "application/x-www-form-urlencoded",
            "multipart-form" => "multipart/form-data",
            _ => "text/plain",
        };
        let mut value = json!({"type": media});
        if body.is_form() {
            value["parameters"] = json!(body.fields().iter().map(parameter_json).collect::<Vec<_>>());
        } else if body.kind == "graphql" {
            value["graphql"] = serde_json::to_value(&body.data)?;
        } else {
            value["data"] = json!(body.as_text());
        }
        api["requestBody"] = value;
    }
    let mut responses = Vec::new();
    let mut examples = Vec::new();
    for (index, example) in request.extra.get("examples").and_then(serde_yaml_ng::Value::as_sequence).into_iter().flatten().enumerate() {
        let response = &example["response"];
        if let Some(status) = response["status"].as_u64() {
            responses.push(json!({"id": index, "code": status, "name": example["name"].as_str().unwrap_or("Response"), "contentType": response["body"]["type"].as_str().unwrap_or("raw")}));
            examples.push(json!({"responseId": index, "name": example["name"].as_str().unwrap_or("Response"), "data": response["body"]["data"].as_str().unwrap_or("")}));
        }
    }
    api["responses"] = responses.into();
    api["responseExamples"] = examples.into();
    Ok(json!({"name": request.info.name, "api": api}))
}

fn parameter_json(parameter: &KeyValue) -> Json {
    let value = if parameter.name.eq_ignore_ascii_case("authorization") || parameter.name.eq_ignore_ascii_case("proxy-authorization") {
        "{{process.env.AUTHORIZATION}}"
    } else if parameter.name.eq_ignore_ascii_case("x-api-key") {
        "{{process.env.API_KEY}}"
    } else {
        &parameter.value
    };
    json!({"name": parameter.name, "value": value, "example": value, "enable": !parameter.disabled,
        "required": parameter.kind.as_deref() == Some("path"), "type": if parameter.kind.as_deref() == Some("file") { "file" } else { "string" }, "description": parameter.description})
}
