//! Sending one request: what it inherits from its folders and the collection, its scripts, the
//! variables, auth, and the response with its tests.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use indexmap::IndexMap;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::Value as Json;

use crate::collection::Collection;
use crate::format::{Auth, AuthConfig, Body, Defaults, KeyValue, RequestFile, Script};
use crate::oauth2::{self, Interact, Placement};
use crate::script::{self, Header, LogLine, ReqState, ResState, ScriptState, TestResult};
use crate::session::Session;
use crate::vars::{Scope, Vars};

/// Everything a send needs, taken from the collection up front so the send can run on its own.
#[derive(Debug, Clone)]
pub struct Plan {
    pub path: PathBuf,
    pub name: String,
    pub collection_name: String,
    pub root: PathBuf,
    pub file: RequestFile,
    /// The collection's defaults, then each folder's from the outermost in.
    pub chain: Vec<(String, Defaults)>,
    pub env_name: Option<String>,
    pub env_vars: Vars,
    pub dotenv: Vars,
}

impl Plan {
    /// The request at `path` as it is on disk.
    pub fn new(collection: &Collection, path: &Path, env: Option<&str>) -> Result<Self> {
        let request = collection
            .request(path)
            .with_context(|| format!("no request at {}", path.display()))?;
        Self::with_file(collection, path, request.doc.data.clone(), env)
    }

    /// The request at `path` with `file` in place of what is saved, for sending unsaved edits.
    pub fn with_file(
        collection: &Collection,
        path: &Path,
        file: RequestFile,
        env: Option<&str>,
    ) -> Result<Self> {
        let mut chain = vec![("collection".to_string(), collection.defaults())];
        for folder in collection.ancestors(path) {
            chain.push((format!("folder {}", folder.name()), folder.defaults()));
        }
        let environment = env.and_then(|name| collection.environment(name));
        if let (Some(name), None) = (env, environment) {
            bail!("there is no environment `{name}`");
        }
        let name = if file.info.name.is_empty() {
            crate::collection::file_stem(path)
        } else {
            file.info.name.clone()
        };
        Ok(Self {
            path: path.to_path_buf(),
            name,
            collection_name: collection.name(),
            root: collection.root.clone(),
            file,
            chain,
            env_name: environment.map(|e| e.name()),
            env_vars: environment.map(|e| e.variables()).unwrap_or_default(),
            dotenv: collection.dotenv.clone(),
        })
    }

    /// The auth that applies: the request's own, or the nearest folder's or the collection's when it
    /// inherits.
    pub fn auth(&self) -> Auth {
        let own = self
            .file
            .http
            .as_ref()
            .and_then(|h| h.auth.clone())
            .unwrap_or_default();
        if own.mode() != "inherit" {
            return own;
        }
        for (_, defaults) in self.chain.iter().rev() {
            if let Some(auth) = &defaults.auth {
                if auth.mode() != "inherit" {
                    return auth.clone();
                }
            }
        }
        Auth::Mode("none".into())
    }

    /// The scope before any script ran.
    pub fn scope(&self, session: &Session) -> Scope {
        let state = session.lock();
        let mut environment = self.env_vars.clone();
        if let Some(overrides) = self
            .env_name
            .as_ref()
            .and_then(|name| state.env_overrides.get(name))
        {
            environment.extend(overrides.clone());
        }
        let mut process_env: Vars = std::env::vars().collect();
        process_env.extend(self.dotenv.clone());
        Scope {
            collection: enabled(&self.chain[0].1.variables),
            environment,
            folders: self.chain[1..]
                .iter()
                .map(|(_, d)| enabled(&d.variables))
                .collect(),
            request: enabled(self.file.variables()),
            runtime: state.runtime_strings(),
            process_env,
        }
    }

    fn scripts(&self, kind: &str) -> Vec<(String, String)> {
        let pick = |label: &str, scripts: &[Script]| -> Vec<(String, String)> {
            scripts
                .iter()
                .filter(|s| s.kind == kind)
                .map(|s| (format!("{label} {kind} script"), s.code.clone()))
                .collect()
        };
        let mut levels: Vec<Vec<(String, String)>> = self
            .chain
            .iter()
            .map(|(label, d)| pick(label, &d.scripts))
            .collect();
        levels.push(pick("request", self.file.scripts()));
        // Before the request: outside in. After it: inside out, so the request's own goes first.
        if kind != "before-request" {
            levels.reverse();
        }
        levels.into_iter().flatten().collect()
    }
}

fn enabled(list: &[KeyValue]) -> Vars {
    list.iter()
        .filter(|v| v.enabled())
        .map(|v| (v.name.clone(), v.value.clone()))
        .collect()
}

/// What went out.
#[derive(Debug, Clone, Default)]
pub struct SentRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub elapsed: Duration,
    pub url: String,
    pub version: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn content_type(&self) -> String {
        self.header("content-type")
            .unwrap_or_default()
            .to_lowercase()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn json(&self) -> Option<Json> {
        serde_json::from_slice(&self.body).ok()
    }

    /// The body for reading: JSON indented, the rest as it came.
    pub fn pretty(&self) -> String {
        match self.json() {
            Some(json) if !self.body.is_empty() => {
                serde_json::to_string_pretty(&json).unwrap_or_else(|_| self.text())
            }
            _ => self.text(),
        }
    }

    /// What the highlighter should treat the body as.
    pub fn language(&self) -> &'static str {
        let ct = self.content_type();
        if ct.contains("json") || self.json().is_some() {
            "json"
        } else if ct.contains("html") {
            "html"
        } else if ct.contains("xml") {
            "xml"
        } else if ct.contains("javascript") {
            "javascript"
        } else {
            "text"
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub sent: Option<SentRequest>,
    pub response: Option<Response>,
    /// Why it did not go out or did not come back.
    pub error: Option<String>,
    /// A script's error after the response came, which does not void the response.
    pub script_error: Option<String>,
    pub tests: Vec<TestResult>,
    pub logs: Vec<LogLine>,
    /// A script asked to skip the request.
    pub skipped: bool,
    pub next_request: Option<String>,
    pub stop: bool,
}

impl Outcome {
    fn failed(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn passed(&self) -> bool {
        self.error.is_none() && self.script_error.is_none() && self.tests.iter().all(|t| t.passed)
    }
}

/// Sends `plan`: scripts before, the request, scripts after and tests.
pub async fn send(plan: &Plan, session: &Session, interact: &dyn Interact) -> Outcome {
    let mut outcome = Outcome::default();
    let file = &plan.file;
    let http = file.http();
    let settings = file.settings();
    let auth = plan.auth();
    let base = base_headers(plan);

    // Before the request.
    let mut req = ReqState {
        method: if http.method.is_empty() {
            "GET".into()
        } else {
            http.method.to_uppercase()
        },
        url: http.url.clone(),
        headers: base
            .iter()
            .map(|(name, value)| Header {
                name: name.clone(),
                value: value.clone(),
            })
            .collect(),
        body: http
            .body
            .as_ref()
            .filter(|b| !b.is_form())
            .map(Body::as_text)
            .unwrap_or_default(),
        body_kind: http
            .body
            .as_ref()
            .map(|b| b.kind.clone())
            .unwrap_or_else(|| "none".into()),
        auth_mode: auth.mode().to_string(),
        timeout: settings.timeout.unwrap_or(0),
    };
    let before = plan.scripts("before-request");
    if !before.is_empty() {
        let run = script::run(&before, script_state(plan, session, req.clone(), None));
        outcome.logs.extend(run.logs);
        outcome.tests.extend(run.tests);
        apply_script_state(plan, session, &run.state);
        if let Some(error) = run.error {
            return outcome.failed(error);
        }
        if run.state.skip {
            outcome.skipped = true;
            return outcome;
        }
        req = run.state.req;
    }

    // The request, with every variable in place.
    let scope = plan.scope(session);
    let client = match client(&settings, session) {
        Ok(client) => client,
        Err(e) => return outcome.failed(format!("{e:#}")),
    };
    let built = match build(plan, &http, &req, &auth, &scope, &client, session, interact).await {
        Ok(built) => built,
        Err(e) => return outcome.failed(format!("{e:#}")),
    };
    let sent = built.sent.clone();
    outcome.sent = Some(sent.clone());
    let started = Instant::now();
    let response = match send_built(&client, built, req.timeout).await {
        Ok(mut response) => {
            response.elapsed = started.elapsed();
            response
        }
        Err(e) => return outcome.failed(error_text(&e)),
    };

    // After the response.
    let after: Vec<_> = plan
        .scripts("after-response")
        .into_iter()
        .chain(plan.scripts("tests"))
        .collect();
    if !after.is_empty() {
        let res = ResState {
            status: response.status,
            status_text: response.status_text.clone(),
            headers: response
                .headers
                .iter()
                .map(|(k, v)| (k.to_lowercase(), v.clone()))
                .collect(),
            body: response
                .json()
                .unwrap_or_else(|| Json::String(response.text())),
            response_time: response.elapsed.as_millis() as u64,
            url: response.url.clone(),
            size: response.body.len(),
        };
        let mut req_after = req.clone();
        req_after.url = sent.url.clone();
        let run = script::run(&after, script_state(plan, session, req_after, Some(res)));
        outcome.logs.extend(run.logs);
        outcome.tests.extend(run.tests);
        apply_script_state(plan, session, &run.state);
        outcome.script_error = run.error;
        outcome.next_request = run.state.next_request.clone();
        outcome.stop = run.state.stop;
    }
    outcome.response = Some(response);
    outcome
}

fn error_text(e: &anyhow::Error) -> String {
    let mut text = format!("{e}");
    let mut source = e.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

/// The collection's, the folders' and the request's headers, the nearest winning by name.
fn base_headers(plan: &Plan) -> Vec<(String, String)> {
    let own = plan.file.http().headers;
    let mut out: Vec<(String, String)> = Vec::new();
    for headers in plan
        .chain
        .iter()
        .map(|(_, d)| d.headers.as_slice())
        .chain(std::iter::once(own.as_slice()))
    {
        for header in headers.iter().filter(|h| h.enabled() && !h.name.is_empty()) {
            out.retain(|(name, _)| !name.eq_ignore_ascii_case(&header.name));
            out.push((header.name.clone(), header.value.clone()));
        }
    }
    out
}

fn script_state(
    plan: &Plan,
    session: &Session,
    req: ReqState,
    res: Option<ResState>,
) -> ScriptState {
    let scope = plan.scope(session);
    let state = session.lock();
    let mut folder_vars = Vars::new();
    for layer in &scope.folders {
        folder_vars.extend(layer.clone());
    }
    ScriptState {
        env_name: plan.env_name.clone().unwrap_or_default(),
        env_vars: scope.environment.clone(),
        runtime_vars: state.runtime.clone(),
        collection_vars: scope.collection.clone(),
        folder_vars,
        request_vars: scope.request.clone(),
        process_env: scope.process_env.clone(),
        oauth2: state
            .tokens
            .iter()
            .map(|(id, token)| (id.clone(), token.fields()))
            .collect(),
        request_name: plan.name.clone(),
        collection_name: plan.collection_name.clone(),
        cwd: plan.root.display().to_string(),
        req,
        res,
        ..Default::default()
    }
}

fn apply_script_state(plan: &Plan, session: &Session, state: &ScriptState) {
    let mut session = session.lock();
    session.runtime = state.runtime_vars.clone();
    for name in &state.deleted_vars {
        session.runtime.shift_remove(name);
    }
    if let Some(env) = &plan.env_name {
        if !state.env_changed.is_empty() {
            session
                .env_overrides
                .entry(env.clone())
                .or_default()
                .extend(state.env_changed.clone());
        }
    }
}

fn client(settings: &crate::format::Settings, session: &Session) -> Result<reqwest::Client> {
    let redirects = if settings.follow_redirects.unwrap_or(true) {
        reqwest::redirect::Policy::limited(settings.max_redirects.unwrap_or(5) as usize)
    } else {
        reqwest::redirect::Policy::none()
    };
    Ok(reqwest::Client::builder()
        .redirect(redirects)
        .cookie_provider(session.cookies.clone())
        .user_agent(concat!("Kestrel/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

struct Built {
    sent: SentRequest,
    request: reqwest::RequestBuilder,
}

#[allow(clippy::too_many_arguments)]
async fn build(
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
    let method = reqwest::Method::from_bytes(i(&req.method).trim().to_uppercase().as_bytes())
        .context("that is not an HTTP method")?;

    // URL: path parameters, then query parameters on top of whatever the URL already has.
    let mut url = i(&req.url).trim().to_string();
    if url.is_empty() {
        bail!("the request has no URL");
    }
    let path_params: Vec<(String, String)> = http
        .params
        .iter()
        .filter(|p| p.enabled() && p.kind.as_deref() == Some("path"))
        .map(|p| (p.name.clone(), i(&p.value)))
        .collect();
    url = fill_path_params(&url, &path_params);
    if !url.contains("://") {
        url = format!("http://{url}");
    }
    let mut query: Vec<(String, String)> = http
        .params
        .iter()
        .filter(|p| p.enabled() && p.kind.as_deref() != Some("path"))
        .map(|p| (i(&p.name), i(&p.value)))
        .collect();

    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|h| (i(&h.name), i(&h.value)))
        .collect();
    let set_header = |headers: &mut Vec<(String, String)>, name: &str, value: String| {
        headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        headers.push((name.to_string(), value));
    };

    // Auth.
    if let Auth::Config(config) = auth {
        let config = interpolate_auth(config, scope);
        match config.kind.as_str() {
            "bearer" => {
                let token = config.str("token");
                let prefix = config.str("prefix");
                let prefix = if prefix.is_empty() {
                    "Bearer".to_string()
                } else {
                    prefix
                };
                if !token.is_empty() {
                    set_header(&mut headers, "Authorization", format!("{prefix} {token}"));
                }
            }
            "basic" => {
                let pair = format!("{}:{}", config.str("username"), config.str("password"));
                set_header(
                    &mut headers,
                    "Authorization",
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD.encode(pair)
                    ),
                );
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

    // Body.
    let body_kind = req.body_kind.as_str();
    let mut sent_body = String::new();
    let mut request = client.request(method.clone(), parsed.clone());
    let has_header = |headers: &[(String, String)], name: &str| {
        headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
    };
    match (body_kind, http.body.as_ref()) {
        ("none", _) | (_, None) => {}
        ("form-urlencoded", Some(body)) => {
            let fields: Vec<(String, String)> = body
                .fields()
                .iter()
                .filter(|f| f.enabled())
                .map(|f| (i(&f.name), i(&f.value)))
                .collect();
            sent_body = serde_urlencoded(&fields);
            if !has_header(&headers, "content-type") {
                headers.push((
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded".into(),
                ));
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
                    let bytes = std::fs::read(&path)
                        .with_context(|| format!("read {} for field {name}", path.display()))?;
                    let file_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    sent_body.push_str(&format!("{name}: <file {value}>\n"));
                    form = form.part(
                        name,
                        reqwest::multipart::Part::bytes(bytes).file_name(file_name),
                    );
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
            sent_body = serde_json::to_string_pretty(
                &serde_json::json!({ "query": i(&query_text), "variables": variables }),
            )?;
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
    Ok(Built {
        sent: SentRequest {
            method: method.to_string(),
            url: parsed.to_string(),
            headers,
            body: sent_body,
        },
        request,
    })
}

async fn send_built(_client: &reqwest::Client, built: Built, timeout_ms: u64) -> Result<Response> {
    let mut request = built.request;
    if timeout_ms > 0 {
        request = request.timeout(Duration::from_millis(timeout_ms));
    }
    let response = request.send().await?;
    let status = response.status();
    let version = format!("{:?}", response.version());
    let url = response.url().to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).to_string(),
            )
        })
        .collect();
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

fn interpolate_auth(config: &AuthConfig, scope: &Scope) -> AuthConfig {
    fn walk(value: &serde_yaml_ng::Value, scope: &Scope) -> serde_yaml_ng::Value {
        use serde_yaml_ng::Value;
        match value {
            Value::String(s) => Value::String(scope.interpolate(s)),
            Value::Mapping(map) => Value::Mapping(
                map.iter()
                    .map(|(k, v)| (k.clone(), walk(v, scope)))
                    .collect(),
            ),
            Value::Sequence(seq) => Value::Sequence(seq.iter().map(|v| walk(v, scope)).collect()),
            other => other.clone(),
        }
    }
    AuthConfig {
        kind: config.kind.clone(),
        fields: config
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), walk(v, scope)))
            .collect::<IndexMap<_, _>>(),
    }
}

/// What a path segment may hold as it is; the rest is percent-encoded.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'%');

/// `/tickets/:ticket_id` with `ticket_id` filled in. A parameter without a value stays as `:name`.
pub fn fill_path_params(url: &str, params: &[(String, String)]) -> String {
    let (head, tail) = match url.find(['?', '#']) {
        Some(i) => url.split_at(i),
        None => (url, ""),
    };
    let segments: Vec<String> = head
        .split('/')
        .map(|segment| {
            if let Some(name) = segment.strip_prefix(':') {
                if let Some((_, value)) = params.iter().find(|(n, v)| n == name && !v.is_empty()) {
                    return utf8_percent_encode(value, SEGMENT).to_string();
                }
            }
            segment.to_string()
        })
        .collect();
    format!("{}{tail}", segments.join("/"))
}

fn serde_urlencoded(fields: &[(String, String)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_params_fill_their_segments() {
        let params = vec![
            ("ticket_id".to_string(), "42".to_string()),
            ("draft_id".to_string(), "a b".to_string()),
        ];
        assert_eq!(
            fill_path_params(
                "http://h/api/tickets/:ticket_id/drafts/:draft_id?x=1",
                &params
            ),
            "http://h/api/tickets/42/drafts/a%20b?x=1"
        );
        assert_eq!(
            fill_path_params("http://h/:missing", &params),
            "http://h/:missing"
        );
    }
}
