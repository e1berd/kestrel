use gpui::{Context, Window};
use gpui_component::WindowExt as _;
use gpui_component::input::InputEvent;
use kestrel_core::format::Body;
use kestrel_core::report::report_error;

use super::{RequestTab, TabEvent, code_editor, language_of};

impl RequestTab {
    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        let dirty = self.build_file(cx) != self.saved_file;
        if dirty != self.dirty {
            self.dirty = dirty;
            cx.emit(TabEvent::Changed);
        }
        self.update_preview(cx);
        cx.notify();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.project.read(cx).tree_move_pending {
            window.push_notification(
                gpui_component::notification::Notification::info("Moving collection items; save again in a moment."),
                cx,
            );
            return;
        }
        self.doc.data = self.build_file(cx);
        match self.doc.save() {
            Ok(()) => {
                self.saved_file = self.doc.data.clone();
                self.project.update(cx, |project, _| project.update_request(&self.doc));
                self.dirty = false;
                self.changed_on_disk = false;
                cx.emit(TabEvent::Changed);
            }
            Err(error) => {
                report_error("save a request", &error);
                window.push_notification(gpui_component::notification::Notification::error(format!("Could not save: {error:#}")), cx);
            }
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

    pub(super) fn reload_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.tokenize_url(window, cx);
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
        self.saved_file = self.build_file(cx);
        self.update_preview(cx);
    }
}
