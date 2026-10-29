use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _,
    Pixels, Point, Styled as _, Subscription, Window, anchored, deferred, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, WindowExt as _, h_flex, v_flex};
use volley_core::format::KeyValue;
use volley_core::vars::Origin;
use volley_core::{Plan, VariableHome};

use super::RequestTab;
use super::variable_marks::VariableState;
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{chip, hint};

#[derive(Clone)]
enum Target {
    Home(VariableHome),
    RequestVars,
    ReadOnly,
}

pub(super) struct VariablePopup {
    name: String,
    anchor: Point<Pixels>,
    state: VariableState,
    origin: String,
    target: Target,
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
                window.push_notification(gpui_component::notification::Notification::error(format!("{error:#}")), cx);
                return;
            }
        };
        let value = scope.get(&name).unwrap_or_default();
        let state = match scope.origin(&name) {
            None => VariableState::Missing,
            Some(_) if value.is_empty() => VariableState::Empty,
            Some(_) => VariableState::Set,
        };
        let (origin, target) = self.describe_origin(&name, scope.origin(&name), cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Value").default_value(value));
        input.update(cx, |input, cx| input.focus(window, cx));
        let subscription = cx.subscribe_in(&input, window, |tab, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                tab.save_variable(window, cx);
            }
        });
        self.variable_popup = Some(VariablePopup { name, anchor, state, origin, target, input, _subscription: subscription });
        cx.notify();
    }

    fn describe_origin(&self, name: &str, origin: Option<Origin>, cx: &Context<Self>) -> (String, Target) {
        let project = self.project.read(cx);
        let active_env = project.env.clone();
        match origin {
            Some(Origin::ProcessEnv) => ("From the process environment or .env. Change it there.".into(), Target::ReadOnly),
            Some(Origin::Dynamic) => ("Generated anew on every send.".into(), Target::ReadOnly),
            Some(Origin::Runtime) => ("Set by a script in this session.".into(), Target::ReadOnly),
            Some(Origin::Request) => ("Defined in this request's Vars.".into(), Target::RequestVars),
            Some(Origin::Folder(index)) => {
                let dir = project.collection.ancestors(&self.path).get(index).map(|folder| folder.dir.clone());
                match dir {
                    Some(dir) => {
                        let home = VariableHome::Folder(dir);
                        (format!("Defined in {}.", home.describe()), Target::Home(home))
                    }
                    None => ("Defined in a folder.".into(), Target::ReadOnly),
                }
            }
            Some(Origin::Environment) => {
                let home = VariableHome::Environment(active_env.unwrap_or_default());
                (format!("Defined in {}.", home.describe()), Target::Home(home))
            }
            Some(Origin::Collection) => {
                let home = VariableHome::Collection;
                (format!("Defined in {}.", home.describe()), Target::Home(home))
            }
            None => {
                let elsewhere = environments_defining(&project.collection, name);
                let home = active_env.map(VariableHome::Environment).unwrap_or(VariableHome::Collection);
                let where_now = if elsewhere.is_empty() {
                    "Not defined anywhere.".to_string()
                } else {
                    format!("Only in environments {}, which are not selected.", elsewhere.join(", "))
                };
                (format!("{where_now} Saving puts it in {}.", home.describe()), Target::Home(home))
            }
        }
    }

    fn save_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popup) = self.variable_popup.take() else { return };
        let value = popup.input.read(cx).value().to_string();
        match popup.target {
            Target::ReadOnly => {}
            Target::RequestVars => {
                let mut items = self.vars.read(cx).items(cx);
                set_in(&mut items, &popup.name, &value);
                self.vars.update(cx, |table, cx| table.set_items(&items, window, cx));
                self.changed(cx);
            }
            Target::Home(home) => {
                let saved = self.project.update(cx, |project, cx| {
                    let result = project.collection.set_variable(&home, &popup.name, &value);
                    cx.notify();
                    result
                });
                if let Err(error) = saved {
                    window.push_notification(gpui_component::notification::Notification::error(format!("{error:#}")), cx);
                }
                self.update_preview(cx);
            }
        }
        cx.notify();
    }

    fn close_variable_popup(&mut self, cx: &mut Context<Self>) {
        self.variable_popup = None;
        cx.notify();
    }

    pub(super) fn render_variable_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.variable_popup.as_ref()?;
        let palette = appearance::palette(cx);
        let is_editable = !matches!(popup.target, Target::ReadOnly);
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
                        .child(
                            Button::new("variable-save")
                                .small()
                                .primary()
                                .label("Save")
                                .on_click(cx.listener(|tab, _, window, cx| tab.save_variable(window, cx))),
                        ),
                )
            });
        Some(deferred(anchored().position(popup.anchor).snap_to_window_with_margin(dp(8.)).child(card)).with_priority(1).into_any_element())
    }
}

fn environments_defining(collection: &volley_core::Collection, name: &str) -> Vec<String> {
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
