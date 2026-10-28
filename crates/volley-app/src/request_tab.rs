//! One open request: the editor on the left, the response on the right. Edits stay here until saved;
//! a send uses them as they are.

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Render, StatefulInteractiveElement as _, Styled, Subscription, Task, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, WindowExt as _, h_flex, h_resizable, resizable_panel, v_flex,
};
use serde_yaml_ng::Value as Yaml;
use volley_core::format::{Auth, AuthConfig, Body, Docs, KeyValue, RequestFile, Runtime, Script};
use volley_core::{Document, Outcome, Plan};

use crate::kv_table::{KvChanged, KvTable};
use crate::project::{Project, ProjectEvent};
use crate::style;

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const BODY_KINDS: [(&str, &str); 7] = [
    ("none", "No body"),
    ("json", "JSON"),
    ("text", "Text"),
    ("xml", "XML"),
    ("form-urlencoded", "Form URL encoded"),
    ("multipart-form", "Multipart form"),
    ("graphql", "GraphQL"),
];
const AUTH_MODES: [(&str, &str); 6] = [
    ("inherit", "Inherit"),
    ("none", "No auth"),
    ("bearer", "Bearer token"),
    ("basic", "Basic"),
    ("apikey", "API key"),
    ("oauth2", "OAuth 2.0"),
];
const SCRIPT_KINDS: [(&str, &str); 3] = [("before-request", "Before request"), ("after-response", "After response"), ("tests", "Tests")];
/// Bodies longer than this show cut, so a huge download does not stall the editor.
const BODY_SHOWN: usize = 2 * 1024 * 1024;

fn auth_fields(mode: &str) -> &'static [(&'static str, &'static str)] {
    match mode {
        "bearer" => &[("token", "Token")],
        "basic" => &[("username", "Username"), ("password", "Password")],
        "apikey" => &[("key", "Key"), ("value", "Value"), ("placement", "Add to (header or query)")],
        "oauth2" => &[
            ("flow", "Flow (authorization_code, client_credentials, password)"),
            ("authorizationUrl", "Authorization URL"),
            ("accessTokenUrl", "Access token URL"),
            ("refreshTokenUrl", "Refresh token URL"),
            ("callbackUrl", "Callback URL"),
            ("credentials.clientId", "Client ID"),
            ("credentials.clientSecret", "Client secret"),
            ("scope", "Scope"),
        ],
        _ => &[],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Params,
    Body,
    Headers,
    Auth,
    Vars,
    Scripts,
    Docs,
}

const SECTIONS: [Section; 7] =
    [Section::Params, Section::Body, Section::Headers, Section::Auth, Section::Vars, Section::Scripts, Section::Docs];

#[derive(Clone, Copy, PartialEq)]
enum ResponseSection {
    Body,
    Headers,
    Tests,
    Console,
    Request,
}

const RESPONSE_SECTIONS: [ResponseSection; 5] =
    [ResponseSection::Body, ResponseSection::Headers, ResponseSection::Tests, ResponseSection::Console, ResponseSection::Request];

pub enum TabEvent {
    /// The title, the method or whether there are unsaved edits changed.
    Changed,
}

pub struct RequestTab {
    pub path: PathBuf,
    project: Entity<Project>,
    doc: Document<RequestFile>,
    pub dirty: bool,
    /// The file changed on disk while there were unsaved edits here.
    pub changed_on_disk: bool,
    method: String,
    url: Entity<InputState>,
    params: Entity<KvTable>,
    headers: Entity<KvTable>,
    vars: Entity<KvTable>,
    body_kind: String,
    body: Entity<EditorState>,
    body_fields: Entity<KvTable>,
    graphql_vars: Entity<EditorState>,
    auth_mode: String,
    auth_inputs: Vec<(&'static str, Entity<InputState>)>,
    scripts: Vec<(&'static str, Entity<EditorState>)>,
    script_kind: usize,
    docs: Entity<EditorState>,
    section: Section,
    response_section: ResponseSection,
    outcome: Option<Outcome>,
    response_body: Entity<EditorState>,
    sending: Option<Task<()>>,
    preview: String,
    missing: Vec<String>,
    token_status: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TabEvent> for RequestTab {}

impl RequestTab {
    pub fn new(project: Entity<Project>, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Option<Self> {
        let doc = project.read(cx).collection.request(&path)?.doc.clone();
        let file = doc.data.clone();
        let http = file.http();
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("{{baseUrl}}/path").default_value(http.url.clone()));
        let params = cx.new(|cx| KvTable::new("params", &http.params, window, cx).new_kind("query"));
        let headers = cx.new(|cx| KvTable::new("headers", &http.headers, window, cx).placeholders("Header", "Value"));
        let vars = cx.new(|cx| KvTable::new("vars", file.variables(), window, cx).placeholders("Variable", "Value"));
        let body_kind = http.body.as_ref().map(|b| b.kind.clone()).unwrap_or_else(|| "none".into());
        let (body_text, gql_vars) = match &http.body {
            Some(b) if b.kind == "graphql" => b.graphql(),
            Some(b) if !b.is_form() => (b.as_text(), String::new()),
            _ => (String::new(), String::new()),
        };
        let body = code_editor(language_of(&body_kind), body_text, window, cx);
        let graphql_vars = code_editor("json", gql_vars, window, cx);
        let fields = http.body.as_ref().filter(|b| b.is_form()).map(Body::fields).unwrap_or_default();
        let body_fields = cx.new(|cx| KvTable::new("fields", &fields, window, cx).placeholders("Field", "Value"));
        let scripts = SCRIPT_KINDS
            .iter()
            .map(|(kind, _)| {
                let code = file.scripts().iter().filter(|s| s.kind == *kind).map(|s| s.code.clone()).collect::<Vec<_>>().join("\n\n");
                (*kind, code_editor("javascript", code, window, cx))
            })
            .collect::<Vec<_>>();
        let docs = code_editor("markdown", file.docs.as_ref().map(|d| d.content().to_string()).unwrap_or_default(), window, cx);
        let response_body = code_editor("json", String::new(), window, cx);
        let auth = http.auth.clone().unwrap_or_default();
        let script_kind = scripts.iter().position(|(_, e)| !e.read(cx).value().is_empty()).unwrap_or(0);

        let mut tab = Self {
            path,
            project: project.clone(),
            doc,
            dirty: false,
            changed_on_disk: false,
            method: if http.method.is_empty() { "GET".into() } else { http.method.to_uppercase() },
            url,
            params,
            headers,
            vars,
            body_kind,
            body,
            body_fields,
            graphql_vars,
            auth_mode: auth.mode().to_string(),
            auth_inputs: Vec::new(),
            scripts,
            script_kind,
            docs,
            section: if http.params.is_empty() && http.body.is_some() { Section::Body } else { Section::Params },
            response_section: ResponseSection::Body,
            outcome: None,
            response_body,
            sending: None,
            preview: String::new(),
            missing: Vec::new(),
            token_status: None,
            _subscriptions: Vec::new(),
        };
        tab.build_auth_inputs(auth.config(), window, cx);

        let mut subscriptions = vec![
            cx.subscribe_in(&tab.url, window, |tab, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    tab.sync_path_params(window, cx);
                    tab.changed(cx);
                }
                InputEvent::PressEnter { .. } => tab.send(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&project, window, |tab, _, event: &ProjectEvent, window, cx| match event {
                ProjectEvent::Reloaded => tab.reload_from_disk(window, cx),
                ProjectEvent::EnvironmentChanged => tab.update_preview(cx),
            }),
        ];
        for table in [&tab.params, &tab.headers, &tab.vars, &tab.body_fields] {
            subscriptions.push(cx.subscribe(table, |tab, _, _: &KvChanged, cx| tab.changed(cx)));
        }
        for editor in
            [tab.body.clone(), tab.graphql_vars.clone(), tab.docs.clone()].into_iter().chain(tab.scripts.iter().map(|(_, e)| e.clone()))
        {
            subscriptions.push(cx.subscribe(&editor, |tab, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    tab.changed(cx);
                }
            }));
        }
        tab._subscriptions = subscriptions;
        tab.update_preview(cx);
        Some(tab)
    }

    pub fn title(&self, cx: &App) -> String {
        let _ = cx;
        if self.doc.data.info.name.is_empty() { volley_core::collection::file_stem(&self.path) } else { self.doc.data.info.name.clone() }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    fn build_auth_inputs(&mut self, config: Option<&AuthConfig>, window: &mut Window, cx: &mut Context<Self>) {
        self.auth_inputs = auth_fields(&self.auth_mode)
            .iter()
            .map(|(key, label)| {
                let value = config.filter(|c| c.kind == self.auth_mode).map(|c| c.str(key)).unwrap_or_default();
                let secret = matches!(*key, "password" | "credentials.clientSecret");
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(*label).masked(secret).default_value(value));
                (*key, input)
            })
            .collect();
        let subscriptions: Vec<_> = self
            .auth_inputs
            .iter()
            .map(|(_, input)| {
                cx.subscribe(input, |tab, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        tab.changed(cx)
                    }
                })
            })
            .collect();
        self._subscriptions.extend(subscriptions);
    }

    /// The file as the editor has it now.
    pub fn build_file(&self, cx: &App) -> RequestFile {
        let mut file = self.doc.data.clone();
        let original = file.http.clone().unwrap_or_default();
        let mut http = original.clone();
        http.method = self.method.clone();
        http.url = self.url.read(cx).value().to_string();
        http.params = keep_if_same(&original.params, self.params.read(cx).items(cx));
        http.headers = keep_if_same(&original.headers, self.headers.read(cx).items(cx));
        http.body = self.build_body(original.body.as_ref(), cx);
        http.auth = self.build_auth(original.auth.as_ref(), cx);
        file.http = Some(http);

        let scripts = self.build_scripts(file.scripts(), cx);
        let variables = keep_if_same(file.variables(), self.vars.read(cx).items(cx));
        let extra = file.runtime.as_ref().map(|r| r.extra.clone()).unwrap_or_default();
        file.runtime =
            if scripts.is_empty() && variables.is_empty() && extra.is_empty() { None } else { Some(Runtime { variables, scripts, extra }) };

        let docs = self.docs.read(cx).value().to_string();
        file.docs = match (&file.docs, docs.is_empty()) {
            (None, true) => None,
            (Some(Docs::Rich { kind, .. }), false) => Some(Docs::Rich { content: docs, kind: kind.clone() }),
            (_, false) => Some(Docs::Text(docs)),
            (Some(_), true) => None,
        };
        file
    }

    fn build_body(&self, original: Option<&Body>, cx: &App) -> Option<Body> {
        let kind = self.body_kind.as_str();
        let same_kind = original.filter(|b| b.kind == kind);
        let body = match kind {
            "none" => return original.filter(|b| b.kind == "none").cloned(),
            "form-urlencoded" | "multipart-form" => {
                let fields = self.body_fields.read(cx).items(cx);
                if let Some(o) = same_kind.filter(|o| o.fields() == fields) {
                    return Some(o.clone());
                }
                let mut body =
                    Body { kind: kind.into(), extra: same_kind.map(|o| o.extra.clone()).unwrap_or_default(), ..Default::default() };
                body.set_fields(&fields);
                body
            }
            "graphql" => {
                let query = self.body.read(cx).value().to_string();
                let variables = self.graphql_vars.read(cx).value().to_string();
                if let Some(o) = same_kind.filter(|o| o.graphql() == (query.clone(), variables.clone())) {
                    return Some(o.clone());
                }
                let mut data = serde_yaml_ng::Mapping::new();
                data.insert("query".into(), query.into());
                data.insert("variables".into(), variables.into());
                Body { kind: kind.into(), data: Yaml::Mapping(data), extra: same_kind.map(|o| o.extra.clone()).unwrap_or_default() }
            }
            _ => {
                let text = self.body.read(cx).value().to_string();
                if let Some(o) = same_kind.filter(|o| o.as_text() == text) {
                    return Some(o.clone());
                }
                Body { kind: kind.into(), data: Yaml::String(text), extra: same_kind.map(|o| o.extra.clone()).unwrap_or_default() }
            }
        };
        Some(body)
    }

    fn build_auth(&self, original: Option<&Auth>, cx: &App) -> Option<Auth> {
        let mode = self.auth_mode.as_str();
        if matches!(mode, "inherit" | "none") {
            return match original {
                Some(o) if o.mode() == mode && o.config().is_none() => Some(o.clone()),
                None if mode == "inherit" => None,
                _ => Some(Auth::Mode(mode.into())),
            };
        }
        let mut config = original.and_then(Auth::config).filter(|c| c.kind == mode).cloned().unwrap_or_else(|| AuthConfig::new(mode));
        for (key, input) in &self.auth_inputs {
            let value = input.read(cx).value().to_string();
            if value.is_empty() && config.str(key).is_empty() && !has_path(&config, key) {
                continue;
            }
            set_path(&mut config, key, value);
        }
        Some(Auth::Config(config))
    }

    fn build_scripts(&self, original: &[Script], cx: &App) -> Vec<Script> {
        let mut out = Vec::new();
        for (kind, editor) in &self.scripts {
            let code = editor.read(cx).value().to_string();
            let existing: Vec<&Script> = original.iter().filter(|s| s.kind == *kind).collect();
            let joined = existing.iter().map(|s| s.code.clone()).collect::<Vec<_>>().join("\n\n");
            if code == joined {
                out.extend(existing.into_iter().cloned());
            } else if !code.trim().is_empty() {
                let extra = existing.first().map(|s| s.extra.clone()).unwrap_or_default();
                out.push(Script { kind: kind.to_string(), code, extra });
            }
        }
        // Keep the order the file had them in when nothing moved.
        if out.len() == original.len() && out.iter().all(|s| original.contains(s)) { original.to_vec() } else { out }
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        let mut doc = self.doc.clone();
        doc.data = self.build_file(cx);
        let dirty = doc.is_dirty();
        if dirty != self.dirty {
            self.dirty = dirty;
            cx.emit(TabEvent::Changed);
        }
        self.update_preview(cx);
        cx.notify();
    }

    /// Path parameters follow the `:name` segments of the URL.
    fn sync_path_params(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value().to_string();
        let path = url.split(['?', '#']).next().unwrap_or_default();
        let names: Vec<String> =
            path.split('/').filter_map(|s| s.strip_prefix(':')).filter(|s| !s.is_empty()).map(str::to_string).collect();
        self.params.update(cx, |table, cx| {
            table.retain_kind("path", &names, cx);
            for name in &names {
                table.ensure(KeyValue::new(name.clone(), "").with_kind("path"), window, cx);
            }
        });
    }

    fn update_preview(&mut self, cx: &mut Context<Self>) {
        let file = self.build_file(cx);
        let project = self.project.read(cx);
        let Ok(plan) = Plan::with_file(&project.collection, &self.path, file, project.env.as_deref()) else { return };
        let scope = plan.scope(&project.session);
        let http = plan.file.http();
        let url = scope.interpolate(&http.url);
        let path_params: Vec<(String, String)> = http
            .params
            .iter()
            .filter(|p| p.enabled() && p.kind.as_deref() == Some("path"))
            .map(|p| (p.name.clone(), scope.interpolate(&p.value)))
            .collect();
        let mut preview = volley_core::http::fill_path_params(&url, &path_params);
        let query: Vec<String> = http
            .params
            .iter()
            .filter(|p| p.enabled() && p.kind.as_deref() != Some("path"))
            .map(|p| format!("{}={}", scope.interpolate(&p.name), scope.interpolate(&p.value)))
            .collect();
        if !query.is_empty() {
            preview.push(if preview.contains('?') { '&' } else { '?' });
            preview.push_str(&query.join("&"));
        }
        self.missing = volley_core::vars::names(&preview);
        self.preview = preview;
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.doc.data = self.build_file(cx);
        match self.doc.save() {
            Ok(()) => {
                self.dirty = false;
                self.changed_on_disk = false;
                cx.emit(TabEvent::Changed);
            }
            Err(e) => window.push_notification(gpui_component::notification::Notification::error(format!("Could not save: {e:#}")), cx),
        }
        cx.notify();
    }

    pub fn revert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(request) = self.project.read(cx).collection.request(&self.path) {
            self.doc = request.doc.clone();
        } else {
            self.doc.revert();
        }
        self.load_inputs(window, cx);
        self.dirty = false;
        self.changed_on_disk = false;
        cx.emit(TabEvent::Changed);
        cx.notify();
    }

    /// The collection was read again: a clean tab shows what is on disk now.
    fn reload_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.project.read(cx).collection.request(&self.path) else { return };
        if request.doc.data == self.doc.data {
            return;
        }
        let fresh = request.doc.clone();
        if self.dirty {
            self.changed_on_disk = true;
            cx.notify();
            return;
        }
        self.doc = fresh;
        self.load_inputs(window, cx);
        cx.emit(TabEvent::Changed);
        cx.notify();
    }

    fn load_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let file = self.doc.data.clone();
        let http = file.http();
        self.method = if http.method.is_empty() { "GET".into() } else { http.method.to_uppercase() };
        self.url.update(cx, |input, cx| input.set_value(http.url.clone(), window, cx));
        self.params.update(cx, |t, cx| t.set_items(&http.params, window, cx));
        self.headers.update(cx, |t, cx| t.set_items(&http.headers, window, cx));
        self.vars.update(cx, |t, cx| t.set_items(file.variables(), window, cx));
        self.body_kind = http.body.as_ref().map(|b| b.kind.clone()).unwrap_or_else(|| "none".into());
        let (text, gql_vars) = match &http.body {
            Some(b) if b.kind == "graphql" => b.graphql(),
            Some(b) if !b.is_form() => (b.as_text(), String::new()),
            _ => (String::new(), String::new()),
        };
        self.body = code_editor(language_of(&self.body_kind), text, window, cx);
        self.graphql_vars.update(cx, |e, cx| e.set_value(gql_vars, window, cx));
        let fields = http.body.as_ref().filter(|b| b.is_form()).map(Body::fields).unwrap_or_default();
        self.body_fields.update(cx, |t, cx| t.set_items(&fields, window, cx));
        let auth = http.auth.clone().unwrap_or_default();
        self.auth_mode = auth.mode().to_string();
        self.build_auth_inputs(auth.config(), window, cx);
        for (kind, editor) in &self.scripts {
            let code = file.scripts().iter().filter(|s| s.kind == *kind).map(|s| s.code.clone()).collect::<Vec<_>>().join("\n\n");
            editor.update(cx, |e, cx| e.set_value(code, window, cx));
        }
        let docs = file.docs.as_ref().map(|d| d.content().to_string()).unwrap_or_default();
        self.docs.update(cx, |e, cx| e.set_value(docs, window, cx));
        let body = self.body.clone();
        self._subscriptions.push(cx.subscribe(&body, |tab, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                tab.changed(cx)
            }
        }));
        self.update_preview(cx);
    }

    fn set_method(&mut self, method: &str, cx: &mut Context<Self>) {
        self.method = method.into();
        self.changed(cx);
        cx.emit(TabEvent::Changed);
    }

    fn set_body_kind(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        if kind == self.body_kind {
            return;
        }
        let was_text = !matches!(self.body_kind.as_str(), "none" | "form-urlencoded" | "multipart-form");
        let text = if was_text { self.body.read(cx).value().to_string() } else { String::new() };
        self.body_kind = kind.into();
        self.body = code_editor(language_of(kind), text, window, cx);
        let body = self.body.clone();
        self._subscriptions.push(cx.subscribe(&body, |tab, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                tab.changed(cx)
            }
        }));
        let has_content_type = self.headers.read(cx).items(cx).iter().any(|h| h.name.eq_ignore_ascii_case("content-type"));
        let _ = has_content_type;
        self.changed(cx);
    }

    fn set_auth_mode(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.auth_mode = mode.into();
        let original = self.doc.data.http.as_ref().and_then(|h| h.auth.clone());
        self.build_auth_inputs(original.as_ref().and_then(Auth::config), window, cx);
        self.changed(cx);
    }

    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sending.is_some() {
            return;
        }
        let file = self.build_file(cx);
        let project = self.project.read(cx);
        let plan = match Plan::with_file(&project.collection, &self.path, file, project.env.as_deref()) {
            Ok(plan) => plan,
            Err(e) => {
                self.outcome = Some(Outcome { error: Some(format!("{e:#}")), ..Default::default() });
                cx.notify();
                return;
            }
        };
        let session = project.session.clone();
        let interact = project.interact.clone();
        let task = volley_core::runtime().spawn(async move { volley_core::send(&plan, &session, &interact).await });
        self.sending = Some(cx.spawn_in(window, async move |tab, cx| {
            let outcome = task.await.unwrap_or_else(|e| Outcome { error: Some(format!("the send stopped: {e}")), ..Default::default() });
            let _ = tab.update_in(cx, |tab, window, cx| tab.show_outcome(outcome, window, cx));
        }));
        cx.notify();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.sending = None;
        cx.notify();
    }

    fn show_outcome(&mut self, outcome: Outcome, window: &mut Window, cx: &mut Context<Self>) {
        self.sending = None;
        let (language, text) = match &outcome.response {
            Some(response) => {
                let mut text = response.pretty();
                if text.len() > BODY_SHOWN {
                    let mut cut = BODY_SHOWN;
                    while !text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    text.truncate(cut);
                    text.push_str("\n\n… cut here: the body is larger than 2 MB");
                }
                (response.language(), text)
            }
            None => ("text", String::new()),
        };
        self.response_body = code_editor(language, text, window, cx);
        if outcome.response.is_none() && (outcome.error.is_some() || !outcome.logs.is_empty()) {
            self.response_section = if outcome.logs.is_empty() { ResponseSection::Body } else { ResponseSection::Console };
        } else if outcome.tests.iter().any(|t| !t.passed) || outcome.script_error.is_some() {
            self.response_section = ResponseSection::Tests;
        }
        self.outcome = Some(outcome);
        self.update_preview(cx);
        cx.notify();
    }

    fn fetch_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let file = self.build_file(cx);
        let project = self.project.read(cx);
        let Ok(plan) = Plan::with_file(&project.collection, &self.path, file, project.env.as_deref()) else { return };
        let session = project.session.clone();
        let interact = project.interact.clone();
        self.token_status = Some("Signing in…".into());
        let task = volley_core::runtime().spawn(async move { volley_core::http::fetch_token(&plan, &session, &interact).await });
        cx.spawn_in(window, async move |tab, cx| {
            let status = match task.await {
                Ok(Ok(token)) => match token.expires_at {
                    Some(at) => format!("Got a token, good for {} more minutes", (at - now()) / 60),
                    None => "Got a token".to_string(),
                },
                Ok(Err(e)) => format!("No token: {e:#}"),
                Err(e) => format!("No token: {e}"),
            };
            let _ = tab.update(cx, |tab, cx| {
                tab.token_status = Some(status);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn copy_as_curl(&self, cx: &mut App) {
        let Some(sent) = self.outcome.as_ref().and_then(|o| o.sent.clone()) else { return };
        let mut parts = vec![format!("curl -X {} {}", sent.method, shell_quote(&sent.url))];
        for (k, v) in &sent.headers {
            parts.push(format!("-H {}", shell_quote(&format!("{k}: {v}"))));
        }
        if !sent.body.is_empty() {
            parts.push(format!("--data-raw {}", shell_quote(&sent.body)));
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(parts.join(" \\\n  ")));
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn keep_if_same(original: &[KeyValue], new: Vec<KeyValue>) -> Vec<KeyValue> {
    if original == new.as_slice() { original.to_vec() } else { new }
}

fn has_path(config: &AuthConfig, path: &str) -> bool {
    let mut parts = path.split('.');
    let mut value = config.fields.get(parts.next().unwrap_or_default());
    for part in parts {
        value = value.and_then(|v| v.get(part));
    }
    value.is_some()
}

fn set_path(config: &mut AuthConfig, path: &str, value: String) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() == 1 {
        config.set(path, value);
        return;
    }
    let mut slot = config.fields.entry(parts[0].to_string()).or_insert_with(|| Yaml::Mapping(Default::default()));
    for part in &parts[1..parts.len() - 1] {
        if !slot.is_mapping() {
            *slot = Yaml::Mapping(Default::default());
        }
        slot = slot
            .as_mapping_mut()
            .expect("a mapping")
            .entry(Yaml::String(part.to_string()))
            .or_insert_with(|| Yaml::Mapping(Default::default()));
    }
    if !slot.is_mapping() {
        *slot = Yaml::Mapping(Default::default());
    }
    slot.as_mapping_mut().expect("a mapping").insert(Yaml::String(parts[parts.len() - 1].to_string()), Yaml::String(value));
}

fn language_of(kind: &str) -> &'static str {
    match kind {
        "json" => "json",
        "xml" => "html",
        "graphql" => "graphql",
        _ => "text",
    }
}

fn code_editor<T: 'static>(language: &'static str, text: String, window: &mut Window, cx: &mut Context<T>) -> Entity<EditorState> {
    cx.new(|cx| EditorState::new(window, cx).language(language).line_number(true).soft_wrap(true).default_value(text))
}

impl RequestTab {
    fn render_url_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let method = self.method.clone();
        let sending = self.sending.is_some();
        let color = style::method_color(&method, cx);
        let theme = cx.theme();
        v_flex()
            .gap_1()
            .p_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("method")
                            .small()
                            .outline()
                            .child(div().w(px(56.)).text_color(color).font_weight(FontWeight::BOLD).child(method.clone()))
                            .dropdown_caret(true)
                            .dropdown_menu({
                                let tab = cx.entity().downgrade();
                                move |menu, _, _| {
                                    METHODS.iter().fold(menu, |menu, m| {
                                        let tab = tab.clone();
                                        menu.item(PopupMenuItem::new(*m).checked(*m == method).on_click(move |_, _, cx| {
                                            let _ = tab.update(cx, |tab, cx| tab.set_method(m, cx));
                                        }))
                                    })
                                }
                            }),
                    )
                    .child(div().flex_1().min_w_0().child(Input::new(&self.url).small()))
                    .child(if sending {
                        Button::new("cancel").small().label("Cancel").on_click(cx.listener(|tab, _, _, cx| tab.cancel(cx)))
                    } else {
                        Button::new("send")
                            .small()
                            .primary()
                            .icon(IconName::Play)
                            .label("Send")
                            .tooltip("Send (Ctrl+Enter)")
                            .on_click(cx.listener(|tab, _, window, cx| tab.send(window, cx)))
                    })
                    .child(
                        Button::new("save")
                            .small()
                            .ghost()
                            .label("Save")
                            .tooltip("Save (Ctrl+S)")
                            .disabled(!self.dirty)
                            .on_click(cx.listener(|tab, _, window, cx| tab.save(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(div().truncate().child(self.preview.clone()))
                    .when(!self.missing.is_empty(), |el| {
                        el.child(div().flex_none().text_color(theme.warning).child(format!("not set: {}", self.missing.join(", "))))
                    }),
            )
            .when(self.changed_on_disk, |el| {
                el.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_xs()
                        .text_color(theme.warning)
                        .child("The file changed on disk while you were editing it.")
                        .child(
                            Button::new("take-disk")
                                .xsmall()
                                .ghost()
                                .label("Use the file")
                                .on_click(cx.listener(|tab, _, window, cx| tab.revert(window, cx))),
                        )
                        .child(Button::new("keep-mine").xsmall().ghost().label("Keep mine").on_click(cx.listener(|tab, _, _, cx| {
                            tab.changed_on_disk = false;
                            cx.notify();
                        }))),
                )
            })
    }

    fn section_label(&self, section: Section, cx: &App) -> String {
        let count = |n: usize| if n == 0 { String::new() } else { format!(" {n}") };
        match section {
            Section::Params => format!("Params{}", count(self.params.read(cx).items(cx).len())),
            Section::Body => {
                if self.body_kind == "none" {
                    "Body".into()
                } else {
                    "Body •".into()
                }
            }
            Section::Headers => format!("Headers{}", count(self.headers.read(cx).items(cx).len())),
            Section::Auth => format!(
                "Auth · {}",
                AUTH_MODES.iter().find(|(m, _)| *m == self.auth_mode).map(|(_, l)| *l).unwrap_or(self.auth_mode.as_str())
            ),
            Section::Vars => format!("Vars{}", count(self.vars.read(cx).items(cx).len())),
            Section::Scripts => {
                let n = self.scripts.iter().filter(|(_, e)| !e.read(cx).value().trim().is_empty()).count();
                format!("Scripts{}", count(n))
            }
            Section::Docs => "Docs".into(),
        }
    }

    fn render_request(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let selected = SECTIONS.iter().position(|s| *s == self.section).unwrap_or(0);
        let tabs = TabBar::new("request-sections")
            .underline()
            .small()
            .selected_index(selected)
            .on_click(cx.listener(|tab, index: &usize, _, cx| {
                tab.section = SECTIONS[*index];
                cx.notify();
            }))
            .children(SECTIONS.iter().map(|s| Tab::new().label(self.section_label(*s, cx))));
        let content: AnyElement = match self.section {
            Section::Params => scroll("params-scroll", self.params.clone().into_any_element()),
            Section::Headers => scroll("headers-scroll", self.headers.clone().into_any_element()),
            Section::Vars => v_flex()
                .size_full()
                .gap_2()
                .child(hint("Request variables: they win over folder, environment and collection ones.", cx))
                .child(scroll("vars-scroll", self.vars.clone().into_any_element()))
                .into_any_element(),
            Section::Body => self.render_body(cx),
            Section::Auth => self.render_auth(window, cx),
            Section::Scripts => self.render_scripts(cx),
            Section::Docs => Editor::new(&self.docs).h_full().into_any_element(),
        };
        v_flex().size_full().child(tabs).child(div().flex_1().min_h_0().p_2().child(content)).into_any_element()
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let kind = self.body_kind.clone();
        let label = BODY_KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l).unwrap_or("Body");
        let picker = Button::new("body-kind").small().outline().label(label).dropdown_caret(true).dropdown_menu({
            let tab = cx.entity().downgrade();
            move |menu, _, _| {
                BODY_KINDS.iter().fold(menu, |menu, (k, l)| {
                    let tab = tab.clone();
                    menu.item(PopupMenuItem::new(*l).checked(*k == kind).on_click(move |_, window, cx| {
                        let _ = tab.update(cx, |tab, cx| tab.set_body_kind(k, window, cx));
                    }))
                })
            }
        });
        let content: AnyElement = match self.body_kind.as_str() {
            "none" => hint("This request sends no body.", cx).into_any_element(),
            "form-urlencoded" | "multipart-form" => scroll("fields-scroll", self.body_fields.clone().into_any_element()),
            "graphql" => v_flex()
                .size_full()
                .gap_2()
                .child(div().h(gpui::relative(0.6)).min_h_0().child(Editor::new(&self.body).h_full()))
                .child(div().text_xs().child("Variables"))
                .child(div().flex_1().min_h_0().child(Editor::new(&self.graphql_vars).h_full()))
                .into_any_element(),
            _ => Editor::new(&self.body).h_full().into_any_element(),
        };
        v_flex().size_full().gap_2().child(h_flex().child(picker)).child(div().flex_1().min_h_0().child(content)).into_any_element()
    }

    fn render_auth(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.auth_mode.clone();
        let label = AUTH_MODES.iter().find(|(m, _)| *m == mode).map(|(_, l)| *l).unwrap_or(mode.as_str()).to_string();
        let picker = Button::new("auth-mode").small().outline().label(label).dropdown_caret(true).dropdown_menu({
            let tab = cx.entity().downgrade();
            let mode = mode.clone();
            move |menu, _, _| {
                AUTH_MODES.iter().fold(menu, |menu, (m, l)| {
                    let tab = tab.clone();
                    menu.item(PopupMenuItem::new(*l).checked(*m == mode).on_click(move |_, window, cx| {
                        let _ = tab.update(cx, |tab, cx| tab.set_auth_mode(m, window, cx));
                    }))
                })
            }
        });
        let project = self.project.read(cx);
        let inherited = Plan::with_file(&project.collection, &self.path, self.build_file(cx), None)
            .map(|p| p.auth().mode().to_string())
            .unwrap_or_default();
        let body: AnyElement = match mode.as_str() {
            "inherit" => hint(&format!("Uses the auth of its folder or the collection: {inherited}."), cx).into_any_element(),
            "none" => hint("Sends no credentials.", cx).into_any_element(),
            "oauth2" => v_flex()
                .gap_2()
                .children(self.auth_inputs.iter().map(|(key, input)| field(key, input, cx)))
                .child(hint("PKCE, where the token goes and auto-refresh are kept as the file has them.", cx))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Button::new("get-token")
                                .small()
                                .label("Get a new token")
                                .on_click(cx.listener(|tab, _, window, cx| tab.fetch_token(window, cx))),
                        )
                        .when_some(self.token_status.clone(), |el, status| el.child(div().text_xs().child(status))),
                )
                .into_any_element(),
            _ => v_flex().gap_2().children(self.auth_inputs.iter().map(|(key, input)| field(key, input, cx))).into_any_element(),
        };
        v_flex().gap_3().child(h_flex().child(picker)).child(body).into_any_element()
    }

    fn render_scripts(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = TabBar::new("script-kinds")
            .segmented()
            .small()
            .selected_index(self.script_kind)
            .on_click(cx.listener(|tab, index: &usize, _, cx| {
                tab.script_kind = *index;
                cx.notify();
            }))
            .children(SCRIPT_KINDS.iter().enumerate().map(|(i, (_, label))| {
                let filled = !self.scripts[i].1.read(cx).value().trim().is_empty();
                Tab::new().label(if filled { format!("{label} •") } else { label.to_string() })
            }));
        let editor = &self.scripts[self.script_kind].1;
        v_flex()
            .size_full()
            .gap_2()
            .child(h_flex().child(tabs))
            .child(div().flex_1().min_h_0().child(Editor::new(editor).h_full()))
            .child(hint("bru, req, res, test(), expect() and require('jsonwebtoken') work as in Bruno.", cx))
            .into_any_element()
    }

    fn render_response(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        if self.sending.is_some() {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Sending…")
                .into_any_element();
        }
        let Some(outcome) = &self.outcome else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_1()
                .text_color(theme.muted_foreground)
                .child("Press Send or Ctrl+Enter")
                .child(
                    div().text_xs().child(self.path.strip_prefix(self.project.read(cx).root()).unwrap_or(&self.path).display().to_string()),
                )
                .into_any_element();
        };
        let status: AnyElement = match (&outcome.response, outcome.skipped) {
            (_, true) => div().text_color(theme.muted_foreground).child("Skipped by a script").into_any_element(),
            (Some(r), _) => h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(style::status_color(r.status, cx))
                        .child(format!("{} {}", r.status, r.status_text)),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(style::duration(r.elapsed)))
                .child(div().text_xs().text_color(theme.muted_foreground).child(style::size(r.body.len())))
                .into_any_element(),
            (None, _) => div().font_weight(FontWeight::BOLD).text_color(theme.red).child("No response").into_any_element(),
        };
        let passed = outcome.tests.iter().filter(|t| t.passed).count();
        let labels: Vec<String> = RESPONSE_SECTIONS
            .iter()
            .map(|s| match s {
                ResponseSection::Body => "Body".to_string(),
                ResponseSection::Headers => format!("Headers {}", outcome.response.as_ref().map(|r| r.headers.len()).unwrap_or(0)),
                ResponseSection::Tests if outcome.tests.is_empty() => "Tests".to_string(),
                ResponseSection::Tests => format!("Tests {passed}/{}", outcome.tests.len()),
                ResponseSection::Console if outcome.logs.is_empty() => "Console".to_string(),
                ResponseSection::Console => format!("Console {}", outcome.logs.len()),
                ResponseSection::Request => "Request".to_string(),
            })
            .collect();
        let selected = RESPONSE_SECTIONS.iter().position(|s| *s == self.response_section).unwrap_or(0);
        let tabs = TabBar::new("response-sections")
            .underline()
            .small()
            .selected_index(selected)
            .on_click(cx.listener(|tab, index: &usize, _, cx| {
                tab.response_section = RESPONSE_SECTIONS[*index];
                cx.notify();
            }))
            .children(labels.into_iter().map(|l| Tab::new().label(l)));
        let error = outcome.error.clone().or_else(|| outcome.script_error.clone().map(|e| format!("Script: {e}")));
        let content: AnyElement = match self.response_section {
            ResponseSection::Body => match &outcome.response {
                Some(_) => Editor::new(&self.response_body).h_full().readonly(true).into_any_element(),
                None => div().into_any_element(),
            },
            ResponseSection::Headers => {
                let rows = outcome.response.as_ref().map(|r| r.headers.clone()).unwrap_or_default();
                scroll("res-headers", pairs(rows, cx).into_any_element())
            }
            ResponseSection::Tests => {
                let items = v_flex().gap_1().children(outcome.tests.iter().map(|t| {
                    let color = if t.passed { cx.theme().green } else { cx.theme().red };
                    v_flex()
                        .child(
                            h_flex().gap_2().child(div().text_color(color).child(if t.passed { "✓" } else { "✗" })).child(t.name.clone()),
                        )
                        .when_some(t.error.clone(), |el, e| el.child(div().pl_5().text_xs().text_color(cx.theme().red).child(e)))
                }));
                let empty = outcome.tests.is_empty().then(|| hint("No tests ran. Write them in Scripts → Tests.", cx));
                scroll("tests", v_flex().gap_2().children(empty).child(items).into_any_element())
            }
            ResponseSection::Console => {
                let lines = v_flex().gap_1().font_family("monospace").text_xs().children(outcome.logs.iter().map(|line| {
                    let color = match line.level.as_str() {
                        "error" => cx.theme().red,
                        "warn" => cx.theme().yellow,
                        _ => cx.theme().foreground,
                    };
                    div().text_color(color).child(line.text.clone())
                }));
                let empty = outcome.logs.is_empty().then(|| hint("Nothing was logged. console.log() in a script shows here.", cx));
                scroll("console", v_flex().gap_2().children(empty).child(lines).into_any_element())
            }
            ResponseSection::Request => match &outcome.sent {
                Some(sent) => scroll(
                    "sent",
                    v_flex()
                        .gap_2()
                        .child(div().font_family("monospace").text_xs().child(format!("{} {}", sent.method, sent.url)))
                        .child(pairs(sent.headers.clone(), cx))
                        .when(!sent.body.is_empty(), |el| el.child(div().font_family("monospace").text_xs().child(sent.body.clone())))
                        .child(
                            h_flex().child(
                                Button::new("copy-curl")
                                    .xsmall()
                                    .ghost()
                                    .icon(IconName::Copy)
                                    .label("Copy as cURL")
                                    .on_click(cx.listener(|tab, _, _, cx| tab.copy_as_curl(cx))),
                            ),
                        )
                        .into_any_element(),
                ),
                None => hint("The request did not go out.", cx).into_any_element(),
            },
        };
        v_flex()
            .size_full()
            .child(h_flex().px_2().py_1().min_h(px(36.)).items_center().justify_between().child(status))
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .mx_2()
                        .mb_1()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().danger.opacity(0.12))
                        .text_sm()
                        .text_color(cx.theme().red)
                        .child(error),
                )
            })
            .child(tabs)
            .child(div().flex_1().min_h_0().p_2().child(content))
            .into_any_element()
    }
}

fn hint(text: &str, cx: &App) -> gpui::Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text.to_string())
}

fn scroll(id: &'static str, content: AnyElement) -> AnyElement {
    div().id(id).size_full().overflow_y_scroll().child(content).into_any_element()
}

fn field(key: &str, input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    let label = key.rsplit('.').next().unwrap_or(key).to_string();
    h_flex()
        .gap_2()
        .items_center()
        .child(div().w(px(130.)).flex_none().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(div().flex_1().child(Input::new(input).small()))
}

fn pairs(rows: Vec<(String, String)>, cx: &App) -> impl IntoElement {
    v_flex().gap_1().text_xs().children(rows.into_iter().map(|(k, v)| {
        h_flex()
            .gap_3()
            .child(div().w(px(200.)).flex_none().text_color(cx.theme().muted_foreground).child(k))
            .child(div().flex_1().min_w_0().font_family("monospace").child(v))
    }))
}

impl Render for RequestTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().size_full().child(self.render_url_bar(cx)).child(div().flex_1().min_h_0().child(
            h_resizable("request-response").child(resizable_panel().child(self.render_request(window, cx))).child(
                resizable_panel().child(div().size_full().border_l_1().border_color(cx.theme().border).child(self.render_response(cx))),
            ),
        ))
    }
}
