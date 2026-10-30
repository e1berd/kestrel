use std::time::Instant;

use anyhow::{Result, bail};
use serde_json::Value as Json;

use crate::format::{Auth, Body};
use crate::oauth2::{self, Interact};
use crate::outcome::Outcome;
use crate::plan::Plan;
use crate::request_builder::{build, interpolate_auth, send_built};
pub use crate::request_builder::{fill_path_params, missing_path_params, path_param_names};
use crate::script::{self, Header, ReqState, ResState, ScriptState};
use crate::session::Session;
use crate::vars::Vars;

pub async fn send(plan: &Plan, session: &Session, interact: &dyn Interact) -> Outcome {
    let mut outcome = Outcome::default();
    let file = &plan.file;
    let http = file.http();
    let settings = file.settings();
    let auth = plan.auth();
    let base = base_headers(plan);

    let mut req = ReqState {
        method: if http.method.is_empty() { "GET".into() } else { http.method.to_uppercase() },
        url: http.url.clone(),
        headers: base.iter().map(|(name, value)| Header { name: name.clone(), value: value.clone() }).collect(),
        body: http.body.as_ref().filter(|b| !b.is_form()).map(Body::as_text).unwrap_or_default(),
        body_kind: http.body.as_ref().map(|b| b.kind.clone()).unwrap_or_else(|| "none".into()),
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

    let after: Vec<_> = plan.scripts("after-response").into_iter().chain(plan.scripts("tests")).collect();
    if !after.is_empty() {
        let res = ResState {
            status: response.status,
            status_text: response.status_text.clone(),
            headers: response.headers.iter().map(|(k, v)| (k.to_lowercase(), v.clone())).collect(),
            body: response.json().unwrap_or_else(|| Json::String(response.text())),
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
pub async fn fetch_token(plan: &Plan, session: &Session, interact: &dyn Interact) -> Result<oauth2::Token> {
    let Auth::Config(config) = plan.auth() else { bail!("the request does not use OAuth2") };
    if config.kind != "oauth2" {
        bail!("the request uses {} auth, not OAuth2", config.kind);
    }
    let config = interpolate_auth(&config, &plan.scope(session));
    let client = client(&plan.file.settings(), session)?;
    oauth2::token(&config, session, &client, interact, true).await
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
fn base_headers(plan: &Plan) -> Vec<(String, String)> {
    let own = plan.file.http().headers;
    let mut out: Vec<(String, String)> = Vec::new();
    for headers in plan.chain.iter().map(|(_, d)| d.headers.as_slice()).chain(std::iter::once(own.as_slice())) {
        for header in headers.iter().filter(|h| h.enabled() && !h.name.is_empty()) {
            out.retain(|(name, _)| !name.eq_ignore_ascii_case(&header.name));
            out.push((header.name.clone(), header.value.clone()));
        }
    }
    out
}
fn script_state(plan: &Plan, session: &Session, req: ReqState, res: Option<ResState>) -> ScriptState {
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
        oauth2: state.tokens.iter().map(|(id, token)| (id.clone(), token.fields())).collect(),
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
    if let Some(env) = &plan.env_name
        && !state.env_changed.is_empty()
    {
        session.env_overrides.entry(env.clone()).or_default().extend(state.env_changed.clone());
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
