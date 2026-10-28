use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use indexmap::IndexMap;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::Value as Json;

use crate::format::{Auth, AuthConfig};
use crate::oauth2::{self, Interact, Placement};
use crate::outcome::{Response, SentRequest};
use crate::plan::Plan;
use crate::script::ReqState;
use crate::session::Session;
use crate::vars::Scope;

pub(crate) struct Built {
    pub(crate) sent: SentRequest,
    pub(crate) request: reqwest::RequestBuilder,
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn build(
    plan: &Plan,
    http: &crate::format::Http,
    req: &ReqState,
    auth: &Auth,
    scope: &Scope,
    client: &reqwest::Client,
    session: &Session,
    interact: &dyn Interact,
) -> Result<Built> {
    let i = |text: &str| scope.interpolate(text);
    let method = reqwest::Method::from_bytes(i(&req.method).trim().to_uppercase().as_bytes()).context("that is not an HTTP method")?;

    let mut url = i(&req.url).trim().to_string();
    if url.is_empty() {
        bail!("the request has no URL");
    }
    let path_params: Vec<(String, String)> =
        http.params.iter().filter(|p| p.enabled() && p.kind.as_deref() == Some("path")).map(|p| (p.name.clone(), i(&p.value))).collect();
    url = fill_path_params(&url, &path_params);
    if !url.contains("://") {
        url = format!("http://{url}");
    }
    let mut query: Vec<(String, String)> =
        http.params.iter().filter(|p| p.enabled() && p.kind.as_deref() != Some("path")).map(|p| (i(&p.name), i(&p.value))).collect();

    let mut headers: Vec<(String, String)> = req.headers.iter().map(|h| (i(&h.name), i(&h.value))).collect();
    let set_header = |headers: &mut Vec<(String, String)>, name: &str, value: String| {
        headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        headers.push((name.to_string(), value));
    };

    if let Auth::Config(config) = auth {
        let config = interpolate_auth(config, scope);
        match config.kind.as_str() {
            "bearer" => {
                let token = config.str("token");
                let prefix = config.str("prefix");
                let prefix = if prefix.is_empty() { "Bearer".to_string() } else { prefix };
                if !token.is_empty() {
                    set_header(&mut headers, "Authorization", format!("{prefix} {token}"));
                }
            }
            "basic" => {
                let pair = format!("{}:{}", config.str("username"), config.str("password"));
                set_header(&mut headers, "Authorization", format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(pair)));
            }
            "apikey" | "api-key" => {
                let key = config.str("key");
                let value = config.str("value");
                if config.str("placement") == "query" || config.str("addTo") == "queryParams" {
                    query.push((key, value));
                } else {
                    set_header(&mut headers, &key, value);
                }
            }
            "oauth2" => {
                let token = oauth2::token(&config, session, client, interact, false).await?;
                match oauth2::placement(&config, &token) {
                    Placement::Header(name, value) => set_header(&mut headers, &name, value),
                    Placement::Query(name, value) => query.push((name, value)),
                }
            }
            "none" | "inherit" => {}
            other => bail!("auth `{other}` is not supported yet"),
        }
    }

    let mut parsed = url::Url::parse(&url).with_context(|| format!("`{url}` is not a URL"))?;
    if !query.is_empty() {
        let mut pairs = parsed.query_pairs_mut();
        for (k, v) in &query {
            pairs.append_pair(k, v);
        }
    }

    let body_kind = req.body_kind.as_str();
    let mut sent_body = String::new();
    let mut request = client.request(method.clone(), parsed.clone());
    let has_header = |headers: &[(String, String)], name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    match (body_kind, http.body.as_ref()) {
        ("none", _) | (_, None) => {}
        ("form-urlencoded", Some(body)) => {
            let fields: Vec<(String, String)> = body.fields().iter().filter(|f| f.enabled()).map(|f| (i(&f.name), i(&f.value))).collect();
            sent_body = serde_urlencoded(&fields);
            if !has_header(&headers, "content-type") {
                headers.push(("Content-Type".into(), "application/x-www-form-urlencoded".into()));
            }
            request = request.body(sent_body.clone());
        }
        ("multipart-form", Some(body)) => {
            let mut form = reqwest::multipart::Form::new();
            for field in body.fields().iter().filter(|f| f.enabled()) {
                let name = i(&field.name);
                let value = i(&field.value);
                if field.kind.as_deref() == Some("file") {
                    let path = plan.root.join(&value);
                    let bytes = std::fs::read(&path).with_context(|| format!("read {} for field {name}", path.display()))?;
                    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    sent_body.push_str(&format!("{name}: <file {value}>\n"));
                    form = form.part(name, reqwest::multipart::Part::bytes(bytes).file_name(file_name));
                } else {
                    sent_body.push_str(&format!("{name}: {value}\n"));
                    form = form.text(name, value);
                }
            }
            request = request.multipart(form);
        }
        ("graphql", Some(body)) => {
            let (query_text, variables) = body.graphql();
            let variables: Json = serde_json::from_str(&i(&variables)).unwrap_or(Json::Null);
            sent_body = serde_json::to_string_pretty(&serde_json::json!({ "query": i(&query_text), "variables": variables }))?;
            if !has_header(&headers, "content-type") {
                headers.push(("Content-Type".into(), "application/json".into()));
            }
            request = request.body(sent_body.clone());
        }
        (kind, Some(_)) => {
            sent_body = i(&req.body);
            let content_type = match kind {
                "json" => "application/json",
                "xml" => "application/xml",
                "sparql" => "application/sparql-query",
                _ => "text/plain",
            };
            if !has_header(&headers, "content-type") {
                headers.push(("Content-Type".into(), content_type.into()));
            }
            request = request.body(sent_body.clone());
        }
    }

    for (name, value) in &headers {
        request = request.header(name.as_str(), value.as_str());
    }
    Ok(Built { sent: SentRequest { method: method.to_string(), url: parsed.to_string(), headers, body: sent_body }, request })
}
pub(crate) async fn send_built(_client: &reqwest::Client, built: Built, timeout_ms: u64) -> Result<Response> {
    let mut request = built.request;
    if timeout_ms > 0 {
        request = request.timeout(Duration::from_millis(timeout_ms));
    }
    let response = request.send().await?;
    let status = response.status();
    let version = format!("{:?}", response.version());
    let url = response.url().to_string();
    let headers = response.headers().iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).to_string())).collect();
    let body = response.bytes().await?.to_vec();
    Ok(Response {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body,
        elapsed: Duration::ZERO,
        url,
        version,
    })
}
pub(crate) fn interpolate_auth(config: &AuthConfig, scope: &Scope) -> AuthConfig {
    fn walk(value: &serde_yaml_ng::Value, scope: &Scope) -> serde_yaml_ng::Value {
        use serde_yaml_ng::Value;
        match value {
            Value::String(s) => Value::String(scope.interpolate(s)),
            Value::Mapping(map) => Value::Mapping(map.iter().map(|(k, v)| (k.clone(), walk(v, scope))).collect()),
            Value::Sequence(seq) => Value::Sequence(seq.iter().map(|v| walk(v, scope)).collect()),
            other => other.clone(),
        }
    }
    AuthConfig {
        kind: config.kind.clone(),
        fields: config.fields.iter().map(|(k, v)| (k.clone(), walk(v, scope))).collect::<IndexMap<_, _>>(),
    }
}
const SEGMENT: &AsciiSet =
    &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>').add(b'?').add(b'`').add(b'{').add(b'}').add(b'/').add(b'%');
pub fn fill_path_params(url: &str, params: &[(String, String)]) -> String {
    let (head, tail) = match url.find(['?', '#']) {
        Some(i) => url.split_at(i),
        None => (url, ""),
    };
    let segments: Vec<String> = head
        .split('/')
        .map(|segment| {
            if let Some(name) = segment.strip_prefix(':')
                && let Some((_, value)) = params.iter().find(|(n, v)| n == name && !v.is_empty())
            {
                return utf8_percent_encode(value, SEGMENT).to_string();
            }
            segment.to_string()
        })
        .collect();
    format!("{}{tail}", segments.join("/"))
}
pub(crate) fn serde_urlencoded(fields: &[(String, String)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_params_fill_their_segments() {
        let params = vec![("ticket_id".to_string(), "42".to_string()), ("draft_id".to_string(), "a b".to_string())];
        assert_eq!(
            fill_path_params("http://h/api/tickets/:ticket_id/drafts/:draft_id?x=1", &params),
            "http://h/api/tickets/42/drafts/a%20b?x=1"
        );
        assert_eq!(fill_path_params("http://h/:missing", &params), "http://h/:missing");
    }
}
