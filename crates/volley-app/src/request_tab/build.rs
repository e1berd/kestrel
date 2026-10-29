use gpui::{App, Context, Window};
use serde_yaml_ng::Value as Yaml;
use volley_core::Plan;
use volley_core::format::{Auth, AuthConfig, Body, Docs, KeyValue, RequestFile, Runtime, Script};

use super::RequestTab;

impl RequestTab {
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

    pub(super) fn build_body(&self, original: Option<&Body>, cx: &App) -> Option<Body> {
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

    pub(super) fn build_auth(&self, original: Option<&Auth>, cx: &App) -> Option<Auth> {
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

    pub(super) fn build_scripts(&self, original: &[Script], cx: &App) -> Vec<Script> {
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
        if out.len() == original.len() && out.iter().all(|s| original.contains(s)) { original.to_vec() } else { out }
    }

    pub(super) fn sync_path_params(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn update_preview(&mut self, cx: &mut Context<Self>) {
        self.refresh_variable_marks(cx);
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
