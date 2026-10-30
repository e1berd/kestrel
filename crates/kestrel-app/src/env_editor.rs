use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render,
    StatefulInteractiveElement as _, Styled, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::notification::Notification;
use gpui_component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use kestrel_core::Document;
use kestrel_core::collection::{ENVIRONMENTS_DIR, sanitize_file_name};
use kestrel_core::format::EnvironmentFile;
use kestrel_core::report::report_error;

use crate::appearance::dp;
use crate::kv_table::KvTable;
use crate::project::{Project, ProjectEvent};

pub struct EnvEditor {
    project: Entity<Project>,
    selected: usize,
    table: Entity<KvTable>,
    new_name: Entity<InputState>,
    select_after_reload: Option<String>,
    _subscription: gpui::Subscription,
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
        Self { project, selected, table, new_name, select_after_reload: None, _subscription: subscription }
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = index;
        let items = self.project.read(cx).collection.environments.get(index).map(|e| e.doc.data.variables.clone()).unwrap_or_default();
        self.table.update(cx, |t, cx| t.set_items(&items, window, cx));
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

impl Render for EnvEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let names: Vec<String> = self.project.read(cx).collection.environments.iter().map(|e| e.name()).collect();
        let active = self.project.read(cx).env.clone();
        let theme = cx.theme();
        h_flex()
            .h(dp(440.))
            .gap_3()
            .child(
                v_flex()
                    .w(dp(170.))
                    .flex_none()
                    .gap_1()
                    .children(names.iter().enumerate().map(|(i, name)| {
                        let in_use = active.as_deref() == Some(name.as_str());
                        div()
                            .id(("env", i))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .text_sm()
                            .when(i == self.selected, |el| el.bg(theme.list_active))
                            .hover(|el| el.bg(theme.accent))
                            .child(if in_use { format!("{name} · in use") } else { name.clone() })
                            .on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
                    }))
                    .child(div().flex_1())
                    .child(Input::new(&self.new_name).small())
                    .child(
                        Button::new("env-add")
                            .small()
                            .label("Add environment")
                            .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .when(names.is_empty(), |el| {
                        el.child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("No environments yet. Add one on the left; it is saved in environments/."),
                        )
                    })
                    .when(!names.is_empty(), |el| {
                        el.child(
                            div().text_sm().font_weight(FontWeight::SEMIBOLD).child(names.get(self.selected).cloned().unwrap_or_default()),
                        )
                        .child(div().id("env-scroll").flex_1().min_h_0().overflow_y_scroll().child(self.table.clone()))
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Secrets belong in .env (git ignores it): {{process.env.NAME}}."),
                                )
                                .child(
                                    Button::new("env-save")
                                        .small()
                                        .primary()
                                        .label("Save")
                                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                                ),
                        )
                    }),
            )
    }
}
