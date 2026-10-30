use std::path::PathBuf;

use gpui::{AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, Styled, Subscription, Task, Window, div};
use gpui_component::input::{EditorState, InputEvent, InputState, TextDecorationCollection};
use gpui_component::{h_resizable, resizable_panel, v_flex};
use kestrel_core::format::{Auth, AuthConfig, Body, RequestFile};
use kestrel_core::{Document, Outcome};

use crate::kv_table::{KvChanged, KvTable};
use crate::project::{Project, ProjectEvent};
mod build;
mod path_params;
mod persistence;
mod relocation;
mod response;
mod response_sections;
mod sections;
mod sending;
mod url_bar;
mod variable_marks;
mod variable_popup;
mod variable_saving;

const METHODS: [&str; 8] = ["GET", "QUERY", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const BODY_KINDS: [(&str, &str); 7] = [
    ("none", "None"),
    ("json", "JSON"),
    ("text", "Text"),
    ("xml", "XML"),
    ("form-urlencoded", "Form"),
    ("multipart-form", "Multipart"),
    ("graphql", "GraphQL"),
];
const AUTH_MODES: [(&str, &str); 6] =
    [("inherit", "Inherit"), ("none", "None"), ("bearer", "Bearer"), ("basic", "Basic"), ("apikey", "API key"), ("oauth2", "OAuth 2.0")];
const SCRIPT_KINDS: [(&str, &str); 3] = [("before-request", "Before request"), ("after-response", "After response"), ("tests", "Tests")];
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
    Changed,
}
pub struct RequestTab {
    pub path: PathBuf,
    project: Entity<Project>,
    doc: Document<RequestFile>,
    saved_file: RequestFile,
    pub dirty: bool,
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
    missing_path_params: Vec<String>,
    token_status: Option<String>,
    variable_popup: Option<variable_popup::VariablePopup>,
    pending_variables: Vec<String>,
    editor_marks: Vec<(gpui::EntityId, TextDecorationCollection)>,
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
            saved_file: file.clone(),
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
            missing_path_params: Vec::new(),
            token_status: None,
            variable_popup: None,
            pending_variables: Vec::new(),
            editor_marks: Vec::new(),
            _subscriptions: Vec::new(),
        };
        tab.build_auth_inputs(auth.config(), window, cx);
        tab.tokenize_url(window, cx);
        tab.saved_file = tab.build_file(cx);

        let mut subscriptions = vec![
            crate::appearance::observe_palette(cx, |tab, cx| tab.refresh_variable_marks(cx)),
            cx.subscribe_in(&tab.url, window, |tab, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    tab.tokenize_url(window, cx);
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

    pub fn title(&self) -> String {
        if self.doc.data.info.name.is_empty() { kestrel_core::collection::file_stem(&self.path) } else { self.doc.data.info.name.clone() }
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
        self.changed(cx);
    }

    fn set_auth_mode(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.auth_mode = mode.into();
        let original = self.doc.data.http.as_ref().and_then(|h| h.auth.clone());
        self.build_auth_inputs(original.as_ref().and_then(Auth::config), window, cx);
        self.changed(cx);
    }
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
impl Render for RequestTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::appearance::palette(cx);
        v_flex().size_full().children(self.render_variable_popup(cx)).child(self.render_url_bar(cx)).child(
            div().flex_1().min_h_0().border_t_1().border_color(palette.border_soft).child(
                h_resizable("request-response").child(resizable_panel().child(self.render_request(window, cx))).child(
                    resizable_panel()
                        .child(div().size_full().border_l_1().border_color(palette.border_soft).child(self.render_response(cx))),
                ),
            ),
        )
    }
}
