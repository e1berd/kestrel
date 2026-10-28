use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, App, Context, IntoElement, ParentElement as _, Styled, Window, div};
use gpui_component::button::Button;
use gpui_component::input::Editor;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{Sizable as _, h_flex, v_flex};
use volley_core::Plan;

use super::{AUTH_MODES, BODY_KINDS, RequestTab, SCRIPT_KINDS, SECTIONS, Section};
use crate::ui::ReportUnlessGone as _;
use crate::ui::*;

impl RequestTab {
    pub(super) fn section_label(&self, section: Section, cx: &App) -> String {
        let count = |n: usize| if n == 0 { String::new() } else { format!(" {n}") };
        match section {
            Section::Params => format!("Params{}", count(self.params.read(cx).items(cx).len())),
            Section::Body => {
                if self.body_kind == "none" {
                    "Body".into()
                } else {
                    "Body •".into()
                }
            }
            Section::Headers => format!("Headers{}", count(self.headers.read(cx).items(cx).len())),
            Section::Auth => format!(
                "Auth · {}",
                AUTH_MODES.iter().find(|(m, _)| *m == self.auth_mode).map(|(_, l)| *l).unwrap_or(self.auth_mode.as_str())
            ),
            Section::Vars => format!("Vars{}", count(self.vars.read(cx).items(cx).len())),
            Section::Scripts => {
                let n = self.scripts.iter().filter(|(_, e)| !e.read(cx).value().trim().is_empty()).count();
                format!("Scripts{}", count(n))
            }
            Section::Docs => "Docs".into(),
        }
    }

    pub(super) fn render_request(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let selected = SECTIONS.iter().position(|s| *s == self.section).unwrap_or(0);
        let tabs = TabBar::new("request-sections")
            .underline()
            .small()
            .selected_index(selected)
            .on_click(cx.listener(|tab, index: &usize, _, cx| {
                tab.section = SECTIONS[*index];
                cx.notify();
            }))
            .children(SECTIONS.iter().map(|s| Tab::new().label(self.section_label(*s, cx))));
        let content: AnyElement = match self.section {
            Section::Params => scroll("params-scroll", self.params.clone().into_any_element()),
            Section::Headers => scroll("headers-scroll", self.headers.clone().into_any_element()),
            Section::Vars => v_flex()
                .size_full()
                .gap_2()
                .child(hint("Request variables: they win over folder, environment and collection ones.", cx))
                .child(scroll("vars-scroll", self.vars.clone().into_any_element()))
                .into_any_element(),
            Section::Body => self.render_body(cx),
            Section::Auth => self.render_auth(window, cx),
            Section::Scripts => self.render_scripts(cx),
            Section::Docs => Editor::new(&self.docs).h_full().into_any_element(),
        };
        v_flex().size_full().child(div().px_2().child(tabs)).child(div().flex_1().min_h_0().p_2().child(content)).into_any_element()
    }

    pub(super) fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let kind = self.body_kind.clone();
        let label = BODY_KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l).unwrap_or("Body");
        let picker = Button::new("body-kind").small().outline().label(label).dropdown_caret(true).dropdown_menu({
            let tab = cx.entity().downgrade();
            move |menu, _, _| {
                BODY_KINDS.iter().fold(menu, |menu, (k, l)| {
                    let tab = tab.clone();
                    menu.item(PopupMenuItem::new(*l).checked(*k == kind).on_click(move |_, window, cx| {
                        tab.update(cx, |tab, cx| tab.set_body_kind(k, window, cx)).report_unless_gone("change the body type");
                    }))
                })
            }
        });
        let content: AnyElement = match self.body_kind.as_str() {
            "none" => hint("This request sends no body.", cx).into_any_element(),
            "form-urlencoded" | "multipart-form" => scroll("fields-scroll", self.body_fields.clone().into_any_element()),
            "graphql" => v_flex()
                .size_full()
                .gap_2()
                .child(div().h(gpui::relative(0.6)).min_h_0().child(Editor::new(&self.body).h_full()))
                .child(div().text_xs().child("Variables"))
                .child(div().flex_1().min_h_0().child(Editor::new(&self.graphql_vars).h_full()))
                .into_any_element(),
            _ => Editor::new(&self.body).h_full().into_any_element(),
        };
        v_flex().size_full().gap_2().child(h_flex().child(picker)).child(div().flex_1().min_h_0().child(content)).into_any_element()
    }

    pub(super) fn render_auth(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.auth_mode.clone();
        let label = AUTH_MODES.iter().find(|(m, _)| *m == mode).map(|(_, l)| *l).unwrap_or(mode.as_str()).to_string();
        let picker = Button::new("auth-mode").small().outline().label(label).dropdown_caret(true).dropdown_menu({
            let tab = cx.entity().downgrade();
            let mode = mode.clone();
            move |menu, _, _| {
                AUTH_MODES.iter().fold(menu, |menu, (m, l)| {
                    let tab = tab.clone();
                    menu.item(PopupMenuItem::new(*l).checked(*m == mode).on_click(move |_, window, cx| {
                        tab.update(cx, |tab, cx| tab.set_auth_mode(m, window, cx)).report_unless_gone("change the auth");
                    }))
                })
            }
        });
        let project = self.project.read(cx);
        let inherited = Plan::with_file(&project.collection, &self.path, self.build_file(cx), None)
            .map(|p| p.auth().mode().to_string())
            .unwrap_or_default();
        let body: AnyElement = match mode.as_str() {
            "inherit" => hint(&format!("Uses the auth of its folder or the collection: {inherited}."), cx).into_any_element(),
            "none" => hint("Sends no credentials.", cx).into_any_element(),
            "oauth2" => v_flex()
                .gap_2()
                .children(self.auth_inputs.iter().map(|(key, input)| labelled_input(key.rsplit('.').next().unwrap_or(key), input, cx)))
                .child(hint("PKCE, where the token goes and auto-refresh are kept as the file has them.", cx))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Button::new("get-token")
                                .small()
                                .label("Get a new token")
                                .on_click(cx.listener(|tab, _, window, cx| tab.fetch_token(window, cx))),
                        )
                        .when_some(self.token_status.clone(), |el, status| el.child(div().text_xs().child(status))),
                )
                .into_any_element(),
            _ => v_flex()
                .gap_2()
                .children(self.auth_inputs.iter().map(|(key, input)| labelled_input(key.rsplit('.').next().unwrap_or(key), input, cx)))
                .into_any_element(),
        };
        v_flex().gap_3().child(h_flex().child(picker)).child(body).into_any_element()
    }

    pub(super) fn render_scripts(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = TabBar::new("script-kinds")
            .segmented()
            .small()
            .selected_index(self.script_kind)
            .on_click(cx.listener(|tab, index: &usize, _, cx| {
                tab.script_kind = *index;
                cx.notify();
            }))
            .children(SCRIPT_KINDS.iter().enumerate().map(|(i, (_, label))| {
                let filled = !self.scripts[i].1.read(cx).value().trim().is_empty();
                Tab::new().label(if filled { format!("{label} •") } else { label.to_string() })
            }));
        let editor = &self.scripts[self.script_kind].1;
        v_flex()
            .size_full()
            .gap_2()
            .child(h_flex().child(tabs))
            .child(div().flex_1().min_h_0().child(Editor::new(editor).h_full()))
            .child(hint("bru, req, res, test(), expect() and require('jsonwebtoken') work as in Bruno.", cx))
            .into_any_element()
    }
}
