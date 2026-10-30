use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _,
    Pixels, Point, Styled as _, Subscription, Window, anchored, deferred, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _, DropdownButton};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::menu::PopupMenuItem;
use gpui_component::notification::Notification;
use gpui_component::{Sizable as _, WindowExt as _, h_flex, v_flex};
use kestrel_core::format::KeyValue;
use kestrel_core::vars::Origin;
use kestrel_core::{Plan, VariableHome};

use super::RequestTab;
use super::variable_marks::VariableState;
use super::variable_saving::SaveMode;
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{ReportUnlessGone as _, chip, hint};

pub(super) struct VariablePopup {
    name: String,
    anchor: Point<Pixels>,
    state: VariableState,
    origin: String,
    editable: bool,
    input: Entity<InputState>,
    _subscription: Subscription,
}

impl RequestTab {
    pub(super) fn open_variable_popup(&mut self, name: String, anchor: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let file = self.build_file(cx);
        let project = self.project.read(cx);
        let scope = match Plan::with_file(&project.collection, &self.path, file, project.env.as_deref()) {
            Ok(plan) => plan.scope(&project.session),
            Err(error) => {
                window.push_notification(Notification::error(format!("{error:#}")), cx);
                return;
            }
        };
        let value = scope.get(&name).unwrap_or_default();
        let state = match scope.origin(&name) {
            None => VariableState::Missing,
            Some(_) if value.is_empty() => VariableState::Empty,
            Some(_) => VariableState::Set,
        };
        let (origin, editable) = self.describe_origin(&name, scope.origin(&name), cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Value").default_value(value));
        input.update(cx, |input, cx| input.focus(window, cx));
        let subscription = cx.subscribe_in(&input, window, |tab, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                tab.save_variable(SaveMode::Auto, window, cx);
            }
        });
        self.variable_popup = Some(VariablePopup { name, anchor, state, origin, editable, input, _subscription: subscription });
        cx.notify();
    }

    fn describe_origin(&self, name: &str, origin: Option<Origin>, cx: &Context<Self>) -> (String, bool) {
        let project = self.project.read(cx);
        let active_env = project.env.clone();
        match origin {
            Some(Origin::ProcessEnv) => ("From the process environment or .env. Change it there.".into(), false),
            Some(Origin::Dynamic) => ("Generated anew on every send.".into(), false),
            Some(Origin::Runtime) => ("Set by a script in this session.".into(), false),
            Some(Origin::Request) => ("Defined in this request's Vars.".into(), true),
            Some(Origin::Folder(index)) => {
                let home = project.collection.ancestors(&self.path).get(index).map(|folder| VariableHome::Folder(folder.dir.clone()));
                match home {
                    Some(home) => (format!("Defined in {}. Saving changes this request only.", home.describe()), true),
                    None => ("Defined in a folder.".into(), false),
                }
            }
            Some(Origin::Environment) => {
                let home = VariableHome::Environment(active_env.unwrap_or_default());
                (format!("Defined in {}. Saving changes this request only.", home.describe()), true)
            }
            Some(Origin::Collection) => {
                (format!("Defined in {}. Saving changes this request only.", VariableHome::Collection.describe()), true)
            }
            None => {
                let elsewhere = environments_defining(&project.collection, name);
                let where_now = if elsewhere.is_empty() {
                    "Not defined anywhere.".to_string()
                } else {
                    format!("Only in environments {}, which are not selected.", elsewhere.join(", "))
                };
                let destination = match active_env {
                    Some(env) => format!("Saving the request also adds it to the environment {env}."),
                    None => "Saving adds it to this request.".to_string(),
                };
                (format!("{where_now} {destination}"), true)
            }
        }
    }

    fn save_variable(&mut self, mode: SaveMode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popup) = self.variable_popup.take() else { return };
        if popup.editable {
            let value = popup.input.read(cx).value().to_string();
            self.apply_variable(&popup.name, &value, mode, window, cx);
        }
        cx.notify();
    }

    fn apply_variable(&mut self, name: &str, value: &str, mode: SaveMode, window: &mut Window, cx: &mut Context<Self>) {
        let mut items = self.vars.read(cx).items(cx);
        match mode {
            SaveMode::Environment => {
                let Some(environment) = self.project.read(cx).env.clone() else {
                    window.push_notification(Notification::error("Select an environment first."), cx);
                    return;
                };
                let home = VariableHome::Environment(environment.clone());
                let saved = self.project.update(cx, |project, cx| {
                    let result = project.collection.set_variable(&home, name, value);
                    cx.notify();
                    result
                });
                if let Err(error) = saved {
                    window.push_notification(Notification::error(format!("Could not save {name}: {error:#}")), cx);
                    return;
                }
                self.pending_variables.retain(|pending| pending != name);
                if items.iter().any(|item| item.name == name) {
                    items.retain(|item| item.name != name);
                    self.vars.update(cx, |table, cx| table.set_items(&items, window, cx));
                    self.changed(cx);
                }
                window.push_notification(Notification::success(format!("Saved {name} in the environment {environment}.")), cx);
            }
            SaveMode::Auto | SaveMode::RequestOnly => {
                set_in(&mut items, name, value);
                self.vars.update(cx, |table, cx| table.set_items(&items, window, cx));
                if mode == SaveMode::Auto {
                    self.track_pending_variable(name.to_string());
                } else {
                    self.pending_variables.retain(|pending| pending != name);
                }
                self.changed(cx);
            }
        }
    }

    fn render_variable_save_button(&self, popup: &VariablePopup, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = cx.entity().downgrade();
        let name = popup.name.clone();
        let input = popup.input.clone();
        DropdownButton::new("variable-save")
            .small()
            .primary()
            .button(
                Button::new("variable-save-main")
                    .label("Save")
                    .on_click(cx.listener(|tab, _, window, cx| tab.save_variable(SaveMode::Auto, window, cx))),
            )
            .dropdown_menu(move |menu, _, _| {
                let modes = [("Save to the environment", SaveMode::Environment), ("Save to this request only", SaveMode::RequestOnly)];
                modes.into_iter().fold(menu, |menu, (label, mode)| {
                    let (tab, name, input) = (tab.clone(), name.clone(), input.clone());
                    menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let value = input.read(cx).value().to_string();
                        tab.update(cx, |tab, cx| {
                            tab.variable_popup = None;
                            tab.apply_variable(&name, &value, mode, window, cx);
                            cx.notify();
                        })
                        .report_unless_gone("save a variable");
                    }))
                })
            })
    }

    fn close_variable_popup(&mut self, cx: &mut Context<Self>) {
        self.variable_popup = None;
        cx.notify();
    }

    pub(super) fn render_variable_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.variable_popup.as_ref()?;
        let palette = appearance::palette(cx);
        let is_editable = popup.editable;
        let card = v_flex()
            .occlude()
            .w(dp(360.))
            .p(dp(14.))
            .gap(dp(10.))
            .rounded(dp(12.))
            .border_1()
            .border_color(palette.border)
            .bg(palette.raised)
            .shadow(appearance::elevation::card(cx))
            .on_mouse_down_out(cx.listener(|tab, _, _, cx| tab.close_variable_popup(cx)))
            .on_key_down(cx.listener(|tab, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    tab.close_variable_popup(cx);
                }
            }))
            .child(
                h_flex()
                    .gap(dp(8.))
                    .items_center()
                    .child(div().flex_1().font_family(MONO_FONT).font_weight(FontWeight::SEMIBOLD).child(popup.name.clone()))
                    .child(chip(popup.state.label(), popup.state.color(&palette), cx)),
            )
            .child(div().text_size(rems(0.8)).text_color(palette.muted).child(popup.origin.clone()))
            .child(Input::new(&popup.input).disabled(!is_editable))
            .when(is_editable, |el| {
                el.child(
                    h_flex()
                        .gap(dp(8.))
                        .items_center()
                        .child(div().flex_1().child(hint("Enter saves · Esc closes", cx)))
                        .child(
                            Button::new("variable-cancel")
                                .small()
                                .ghost()
                                .label("Cancel")
                                .on_click(cx.listener(|tab, _, _, cx| tab.close_variable_popup(cx))),
                        )
                        .child(self.render_variable_save_button(popup, cx)),
                )
            });
        Some(deferred(anchored().position(popup.anchor).snap_to_window_with_margin(dp(8.)).child(card)).with_priority(1).into_any_element())
    }
}

pub(super) fn environments_defining(collection: &kestrel_core::Collection, name: &str) -> Vec<String> {
    collection
        .environments
        .iter()
        .filter(|environment| environment.variables().contains_key(name))
        .map(|environment| environment.name())
        .collect()
}

fn set_in(items: &mut Vec<KeyValue>, name: &str, value: &str) {
    match items.iter_mut().find(|item| item.enabled() && item.name == name) {
        Some(item) => item.value = value.to_string(),
        None => items.push(KeyValue { name: name.to_string(), value: value.to_string(), ..Default::default() }),
    }
}
