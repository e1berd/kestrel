use gpui::{App, Context, Window};
use volley_core::{Outcome, Plan};

use super::{BODY_SHOWN, RequestTab, ResponseSection, code_editor};
use crate::ui::ReportUnlessGone as _;

impl RequestTab {
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
            tab.update_in(cx, |tab, window, cx| tab.show_outcome(outcome, window, cx)).report_unless_gone("show the response");
        }));
        cx.notify();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.sending = None;
        cx.notify();
    }

    pub(super) fn show_outcome(&mut self, outcome: Outcome, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn fetch_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            tab.update(cx, |tab, cx| {
                tab.token_status = Some(status);
                cx.notify();
            })
            .report_unless_gone("show the token status");
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
