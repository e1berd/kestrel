use gpui::{Context, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use kestrel_core::VariableHome;

use super::RequestTab;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::request_tab) enum SaveMode {
    Auto,
    Environment,
    RequestOnly,
}

pub(super) struct Promotion {
    environment: Option<String>,
    promoted: Vec<String>,
    kept_in_request: Vec<String>,
}

impl Promotion {
    pub(super) fn announce(&self, window: &mut Window, cx: &mut gpui::App) {
        if let Some(environment) = &self.environment
            && !self.promoted.is_empty()
        {
            let message = format!("Saved {} in the environment {environment}.", self.promoted.join(", "));
            window.push_notification(Notification::success(message), cx);
        }
        if !self.kept_in_request.is_empty() {
            let message = format!("Saved {} in this request only.", self.kept_in_request.join(", "));
            window.push_notification(Notification::success(message), cx);
        }
    }
}

impl RequestTab {
    pub(super) fn track_pending_variable(&mut self, name: String) {
        if !self.pending_variables.contains(&name) {
            self.pending_variables.push(name);
        }
    }

    pub(super) fn promote_pending_variables(&mut self, mode: SaveMode, window: &mut Window, cx: &mut Context<Self>) -> Option<Promotion> {
        let mut items = self.vars.read(cx).items(cx);
        let pending: Vec<String> =
            std::mem::take(&mut self.pending_variables).into_iter().filter(|name| items.iter().any(|item| item.name == *name)).collect();
        let environment = self.project.read(cx).env.clone();
        if mode == SaveMode::Environment && environment.is_none() && !pending.is_empty() {
            self.pending_variables = pending;
            window.push_notification(Notification::error("Select an environment first."), cx);
            return None;
        }
        let (promoted, kept_in_request): (Vec<String>, Vec<String>) =
            pending.iter().cloned().partition(|name| self.should_promote(name, mode, environment.as_deref(), cx));
        if let Some(environment) = &environment {
            for name in &promoted {
                let value = items.iter().find(|item| item.name == *name).map(|item| item.value.clone()).unwrap_or_default();
                let home = VariableHome::Environment(environment.clone());
                let saved = self.project.update(cx, |project, cx| {
                    let result = project.collection.set_variable(&home, name, &value);
                    cx.notify();
                    result
                });
                if let Err(error) = saved {
                    self.pending_variables = pending;
                    window.push_notification(Notification::error(format!("Could not save {name}: {error:#}")), cx);
                    return None;
                }
            }
        }
        if !promoted.is_empty() {
            items.retain(|item| !promoted.contains(&item.name));
            self.vars.update(cx, |table, cx| table.set_items(&items, window, cx));
        }
        Some(Promotion { environment, promoted, kept_in_request })
    }

    fn should_promote(&self, name: &str, mode: SaveMode, environment: Option<&str>, cx: &Context<Self>) -> bool {
        let Some(environment) = environment else { return false };
        match mode {
            SaveMode::Environment => true,
            SaveMode::RequestOnly => false,
            SaveMode::Auto => !self
                .project
                .read(cx)
                .collection
                .environments
                .iter()
                .any(|candidate| candidate.name() == environment && candidate.variables().contains_key(name)),
        }
    }
}
