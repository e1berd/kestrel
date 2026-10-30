use anyhow::{Context as _, Result, bail};
use kestrel_core::format::{Auth, AuthConfig, Body, KeyValue, RequestFile};
use serde_json::{Value as Json, json};

pub(crate) fn convert(item: &Json, api: &Json, spec: &Json, warnings: &mut Vec<String>) -> Result<RequestFile> {
    let method = api["method"].as_str().context("Apidog request has no HTTP method")?.to_lowercase();
    if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)) {
        bail!("unsupported Apidog HTTP method: {method}");
    }
    let path = api["path"].as_str().filter(|path| !path.is_empty()).context("Apidog request has an empty path")?;
    let name = item.get("name").or_else(|| api.get("name")).and_then(Json::as_str).unwrap_or(path);
    let description = item.get("description").or_else(|| api.get("description")).cloned().unwrap_or(Json::Null);
    let mut parameters = Vec::new();
    for kind in ["path", "query", "header", "cookie"] {
        for parameter in api["parameters"][kind].as_array().into_iter().flatten() {
            let mut parameter = parameter.clone();
            parameter["in"] = json!(kind);
            if parameter.get("schema").is_none() {
                parameter["schema"] = json!({"type": parameter.get("type").and_then(Json::as_str).unwrap_or("string")});
            }
            if let Some(value) = parameter.get("value").or_else(|| parameter.get("example")).cloned() {
                parameter["example"] = value;
            }
            parameters.push(parameter);
        }
    }
    let mut operation = json!({"summary": name, "description": description, "parameters": parameters});
    if let Some(tags) = api.get("tags") {
        operation["tags"] = tags.clone();
    }
    let body = &api["requestBody"];
    let media = body.get("type").and_then(Json::as_str).unwrap_or("none");
    if media != "none" {
        operation["requestBody"] = json!({"content": {media: {"schema": body.get("jsonSchema").cloned().unwrap_or(json!({}))}}});
    }
    let mut responses = serde_json::Map::new();
    for response in api.get("responses").and_then(Json::as_array).into_iter().flatten() {
        let Some(code) = response.get("code") else { continue };
        let code = code.as_str().map(str::to_string).unwrap_or_else(|| code.to_string());
        let media = response_media(response.get("contentType").and_then(Json::as_str).unwrap_or("json"));
        let mut content = json!({"schema": response.get("jsonSchema").cloned().unwrap_or(json!({}))});
        if let Some(example) = api
            .get("responseExamples")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .find(|example| example.get("responseId").is_some() && example.get("responseId") == response.get("id"))
            .and_then(|example| example.get("data"))
        {
            content["example"] = if media.contains("json") {
                match example.as_str().map(serde_json::from_str) {
                    Some(Ok(example)) => example,
                    _ => example.clone(),
                }
            } else {
                example.clone()
            };
        }
        responses.insert(
            code,
            json!({"description": response.get("description").or_else(|| response.get("name"))
            .and_then(Json::as_str).unwrap_or("Response"), "content": {media: content}}),
        );
    }
    operation["responses"] = responses.into();
    let mut file = super::generated_request(spec, path, &method, operation)?;
    let http = file.http.as_mut().context("the generated request is not HTTP")?;
    if path.starts_with("https://") || path.starts_with("http://") || path.starts_with("{{") {
        http.url = path.into();
    }
    for kind in ["query", "header", "cookie"] {
        for parameter in api["parameters"][kind].as_array().into_iter().flatten() {
            let name = parameter.get("name").and_then(Json::as_str).unwrap_or_default();
            if kind == "cookie" {
                warnings.push("Cookie parameters need to be configured manually after import.".into());
                continue;
            }
            let values = if kind == "header" { &mut http.headers } else { &mut http.params };
            for value in values.iter_mut().filter(|value| value.name == name) {
                value.disabled = parameter.get("enable").and_then(Json::as_bool) == Some(false);
            }
        }
    }
    if let Some(body) = body_override(body)? {
        http.body = Some(body);
    }
    for header in &mut http.headers {
        if header.name.eq_ignore_ascii_case("authorization") || header.name.eq_ignore_ascii_case("proxy-authorization") {
            header.value = "{{process.env.AUTHORIZATION}}".into();
            warnings.push("Authorization header values are replaced with {{process.env.AUTHORIZATION}}.".into());
        } else if header.name.eq_ignore_ascii_case("x-api-key") {
            header.value = "{{process.env.API_KEY}}".into();
            warnings.push("X-API-Key header values are replaced with {{process.env.API_KEY}}.".into());
        }
    }
    let mut example_request = serde_yaml_ng::to_value(http)?;
    if let Some(request) = example_request.as_mapping_mut() {
        request.remove("auth");
    }
    if let Some(examples) = file.extra.get_mut("examples").and_then(serde_yaml_ng::Value::as_sequence_mut) {
        for example in examples {
            example["request"] = example_request.clone();
        }
    }
    Ok(file)
}

fn response_media(media: &str) -> &str {
    match media {
        "json" => "application/json",
        "xml" => "application/xml",
        "html" => "text/html",
        "eventStream" => "text/event-stream",
        "msgpack" => "application/x-msgpack",
        "binary" => "application/octet-stream",
        "raw" | "noContent" => "text/plain",
        media => media,
    }
}

fn body_override(body: &Json) -> Result<Option<Body>> {
    let media = body.get("type").and_then(Json::as_str).unwrap_or("none");
    if media == "graphql" {
        return Ok(Some(Body { kind: "graphql".into(), data: serde_yaml_ng::to_value(&body["graphql"])?, ..Default::default() }));
    }
    if media == "application/x-www-form-urlencoded" || media == "multipart/form-data" {
        let kind = if media == "multipart/form-data" { "multipart-form" } else { "form-urlencoded" };
        let mut result = Body { kind: kind.into(), ..Default::default() };
        let fields: Vec<_> = body
            .get("parameters")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .map(|parameter| {
                let mut value = KeyValue::new(
                    parameter.get("name").and_then(Json::as_str).unwrap_or_default(),
                    text(parameter.get("value").or_else(|| parameter.get("example")).unwrap_or(&Json::Null)),
                );
                value.disabled = parameter.get("enable").and_then(Json::as_bool) == Some(false);
                value.description = parameter.get("description").and_then(Json::as_str).map(str::to_string);
                if parameter["type"] == "file" {
                    value.kind = Some("file".into());
                }
                value
            })
            .collect();
        if fields.is_empty() {
            return Ok(None);
        }
        result.set_fields(&fields);
        return Ok(Some(result));
    }
    let Some(data) = body.get("data").or_else(|| body.get("example")) else { return Ok(None) };
    let kind = if media.contains("json") {
        "json"
    } else if media.contains("xml") {
        "xml"
    } else {
        "text"
    };
    Ok(Some(Body::text(kind, text(data))))
}

pub(crate) fn auth(auth: &Json, warnings: &mut Vec<String>) -> Option<Auth> {
    let kind = auth.get("type").and_then(Json::as_str)?;
    match kind {
        "inherit" | "" => None,
        "none" | "noauth" => Some(Auth::Mode("none".into())),
        "bearer" | "basic" | "apikey" => {
            warnings.push(
                "Authentication credentials are replaced with process.env placeholders; set them in the collection .env file.".into(),
            );
            let mut config = AuthConfig::new(kind);
            if kind == "bearer" {
                config.set("token", "{{process.env.API_TOKEN}}");
            } else if kind == "basic" {
                config.set("username", "{{process.env.API_USERNAME}}");
                config.set("password", "{{process.env.API_PASSWORD}}");
            } else {
                config.set("key", auth.pointer("/apikey/key").and_then(Json::as_str).unwrap_or("X-API-Key"));
                config.set("value", "{{process.env.API_KEY}}");
                config.set("placement", auth.pointer("/apikey/in").and_then(Json::as_str).unwrap_or("header"));
            }
            Some(Auth::Config(config))
        }
        _ => {
            warnings.push(format!("Apidog authentication type '{kind}' needs to be configured manually after import."));
            Some(Auth::Mode("none".into()))
        }
    }
}

fn text(value: &Json) -> String {
    match value {
        Json::Null => String::new(),
        Json::String(value) => value.clone(),
        value => value.to_string(),
    }
}
