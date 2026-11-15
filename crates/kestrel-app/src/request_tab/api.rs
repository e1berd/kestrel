use gpui::{AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{EditorState, InputEvent};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{Icon, IconName, Sizable as _, h_flex};

use super::{RequestTab, Section, TabEvent};
pub(super) use crate::api_kind::ApiKind;
use crate::appearance::{self, dp};
use crate::rabbitmq_tab::{RabbitMqChanged, RabbitMqTab};
use crate::ui::ReportUnlessGone as _;

pub(super) struct HttpDraft {
    method: String,
    body_kind: String,
    body: Entity<EditorState>,
}

impl RequestTab {
    pub(super) fn load_rabbitmq(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.rabbitmq {
            tab.update(cx, |tab, cx| tab.disconnect(cx));
        }
        let project = self.project.clone();
        let path = self.path.clone();
        let config = self.doc.data.rabbitmq.clone().unwrap_or_default();
        let variables = self.vars.clone();
        let docs = self.docs.clone();
        let editor = cx.new(|cx| RabbitMqTab::new(project, path, config, variables, docs, window, cx));
        self._subscriptions.push(cx.subscribe(&editor, |tab, _, _: &RabbitMqChanged, cx| tab.changed(cx)));
        self._subscriptions.push(cx.observe(&editor, |_, _, cx| cx.notify()));
        self.rabbitmq = Some(editor);
    }

    fn set_api(&mut self, api: ApiKind, window: &mut Window, cx: &mut Context<Self>) {
        if api == self.api {
            return;
        }
        self.cancel(cx);
        if let Some(tab) = &self.rabbitmq {
            tab.update(cx, |tab, cx| tab.disconnect(cx));
        }
        self.api = api;
        match api {
            ApiKind::RabbitMq if self.rabbitmq.is_none() => self.load_rabbitmq(window, cx),
            ApiKind::GraphQl => {
                self.select_http_api(api, window, cx);
                self.section = Section::Body;
            }
            ApiKind::Rest => self.select_http_api(api, window, cx),
            _ => {}
        }
        self.outcome = None;
        self.changed(cx);
        cx.emit(TabEvent::Changed);
    }

    fn select_http_api(&mut self, api: ApiKind, window: &mut Window, cx: &mut Context<Self>) {
        if (self.body_kind == "graphql") == (api == ApiKind::GraphQl) {
            return;
        }
        let current = HttpDraft { method: self.method.clone(), body_kind: self.body_kind.clone(), body: self.body.clone() };
        let draft =
            self.http_draft.take().filter(|draft| (draft.body_kind == "graphql") == (api == ApiKind::GraphQl)).unwrap_or_else(|| {
                let kind = if api == ApiKind::GraphQl { "graphql" } else { "none" };
                let body = super::code_editor(super::language_of(kind), String::new(), window, cx);
                self._subscriptions.push(cx.subscribe(&body, |tab, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        tab.changed(cx);
                    }
                }));
                HttpDraft { method: if api == ApiKind::GraphQl { "POST" } else { "GET" }.into(), body_kind: kind.into(), body }
            });
        self.method = draft.method;
        self.body_kind = draft.body_kind;
        self.body = draft.body;
        self.http_draft = Some(current);
    }

    pub(super) fn render_api_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let selected = self.api;
        let tab = cx.entity().downgrade();
        Button::new("api-kind")
            .ghost()
            .h(gpui::rems(2.8))
            .w(dp(56.))
            .px(dp(10.))
            .flex_none()
            .tooltip(selected.label())
            .accessibility_label(format!("API: {}", selected.label()))
            .rounded(dp(10.))
            .child(
                h_flex()
                    .items_center()
                    .gap(dp(6.))
                    .child(selected.icon(cx))
                    .child(Icon::new(IconName::ChevronDown).xsmall().text_color(palette.faint)),
            )
            .dropdown_menu(move |menu, _, cx| {
                [ApiKind::Rest, ApiKind::GraphQl, ApiKind::RabbitMq].into_iter().fold(
                    menu.check_side(gpui_component::Side::Right),
                    |menu, api| {
                        let tab = tab.clone();
                        menu.item(PopupMenuItem::new(api.label()).icon(api.icon(cx)).checked(api == selected).on_click(
                            move |_, window, cx| {
                                tab.update(cx, |tab, cx| tab.set_api(api, window, cx)).report_unless_gone("select an API type");
                            },
                        ))
                    },
                )
            })
    }

    pub(super) fn render_rabbitmq_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .px(dp(16.))
            .pt(dp(14.))
            .pb(dp(8.))
            .items_center()
            .gap(dp(8.))
            .child(self.render_api_picker(cx))
            .children(self.rabbitmq.as_ref().map(|tab| tab.update(cx, |tab, cx| tab.render_connection_bar(cx))))
            .child(self.render_save_button(cx))
    }
}
