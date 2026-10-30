use gpui::{AppContext as _, Context, Entity, Window};
use gpui_component::WindowExt as _;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::notification::Notification;
use kestrel_core::Document;
use kestrel_core::collection::{ENVIRONMENTS_DIR, sanitize_file_name};
use kestrel_core::format::EnvironmentFile;
use kestrel_core::report::report_error;

mod render;

use crate::dialogs::confirm;
use crate::kv_table::KvTable;
use crate::project::{Project, ProjectEvent};
use crate::ui::ReportUnlessGone as _;

pub struct EnvEditor {
    project: Entity<Project>,
    selected: usize,
    table: Entity<KvTable>,
    new_name: Entity<InputState>,
    select_after_reload: Option<String>,
    _subscriptions: [gpui::Subscription; 2],
}

impl EnvEditor {
    pub fn new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = project.read(cx).env.clone();
        let selected = current.and_then(|name| project.read(cx).collection.environments.iter().position(|e| e.name() == name)).unwrap_or(0);
        let items = project.read(cx).collection.environments.get(selected).map(|e| e.doc.data.variables.clone()).unwrap_or_default();
        let table = cx.new(|cx| KvTable::new("env-vars", &items, window, cx).placeholders("Variable", "Value"));
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("New environment name"));
        let subscription = cx.subscribe_in(&project, window, |editor, _, event: &ProjectEvent, window, cx| {
            if matches!(event, ProjectEvent::Reloaded)
                && let Some(name) = editor.select_after_reload.take()
                && let Some(index) =
                    editor.project.read(cx).collection.environments.iter().position(|environment| environment.name() == name)
            {
                editor.select(index, window, cx);
            }
        });
        let enter = cx.subscribe_in(&new_name, window, |editor, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                editor.add(window, cx);
            }
        });
        Self { project, selected, table, new_name, select_after_reload: None, _subscriptions: [subscription, enter] }
    }

    pub(super) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = index;
        let items = self.project.read(cx).collection.environments.get(index).map(|e| e.doc.data.variables.clone()).unwrap_or_default();
        self.table.update(cx, |t, cx| t.set_items(&items, window, cx));
        cx.notify();
    }

    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.table.read(cx).items(cx);
        let index = self.selected;
        let result = self.project.update(cx, |project, cx| {
            let result = project.edit_collection(|collection| {
                let Some(environment) = collection.environments.get_mut(index) else { return Ok(()) };
                environment.doc.data.variables = items;
                environment.doc.save()
            });
            cx.notify();
            result
        });
        match result {
            Ok(()) => window.push_notification(Notification::success("Environment saved"), cx),
            Err(error) => {
                report_error("edit collection environments", &error);
                window.push_notification(Notification::error(format!("{error:#}")), cx);
            }
        }
    }

    pub(super) fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let result = self.project.update(cx, |project, cx| {
            project.edit_collection(|collection| {
                let path = collection.root.join(ENVIRONMENTS_DIR).join(format!("{}.yml", sanitize_file_name(&name)));
                if path.exists() {
                    return Err(anyhow::anyhow!("there is an environment called {name} already"));
                }
                let variables = collection
                    .environments
                    .first()
                    .map(|e| {
                        e.doc.data.variables.iter().map(|v| kestrel_core::format::KeyValue { value: String::new(), ..v.clone() }).collect()
                    })
                    .unwrap_or_default();
                Document::new(path, EnvironmentFile { name: name.clone(), variables, ..Default::default() }, false).save()
            })?;
            project.reload(cx);
            anyhow::Ok(())
        });
        match result {
            Ok(()) => {
                self.select_after_reload = Some(name);
                self.new_name.update(cx, |input, cx| input.set_value("", window, cx));
            }
            Err(error) => {
                report_error("edit collection environments", &error);
                window.push_notification(Notification::error(format!("{error:#}")), cx);
            }
        }
    }
}

impl EnvEditor {
    pub(super) fn use_selected(&mut self, cx: &mut Context<Self>) {
        let name = self.project.read(cx).collection.environments.get(self.selected).map(|environment| environment.name());
        self.project.update(cx, |project, cx| project.set_env(name, cx));
        cx.notify();
    }
}

impl EnvEditor {
    pub(super) fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.project.read(cx).collection.environments.get(self.selected).map(|environment| environment.name()) else {
            return;
        };
        let editor = cx.entity().downgrade();
        let description = "Its file is deleted from disk. Git still has it if it was committed.";
        confirm(window, cx, format!("Delete environment {name}?"), description, "Delete", move |window, cx| {
            editor.update(cx, |editor, cx| editor.delete_selected(window, cx)).report_unless_gone("delete an environment");
        });
    }

    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.selected;
        let result = self.project.update(cx, |project, cx| {
            let removed = project.edit_collection(|collection| {
                if index >= collection.environments.len() {
                    return Ok(None);
                }
                let environment = collection.environments.remove(index);
                std::fs::remove_file(&environment.doc.path)?;
                anyhow::Ok(Some(environment.name()))
            })?;
            if removed.is_some() && project.env == removed {
                project.set_env(None, cx);
            }
            project.reload(cx);
            anyhow::Ok(())
        });
        match result {
            Ok(()) => {
                let remaining = self.project.read(cx).collection.environments.len();
                self.select(index.min(remaining.saturating_sub(1)), window, cx);
            }
            Err(error) => {
                report_error("delete an environment", &error);
                window.push_notification(Notification::error(format!("{error:#}")), cx);
            }
        }
    }
}
