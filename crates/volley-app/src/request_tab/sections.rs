use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, App, Context, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled, Window, div};
use gpui_component::button::Button;
use gpui_component::input::Editor;
use gpui_component::{Sizable as _, h_flex, v_flex};
use volley_core::Plan;

use super::{AUTH_MODES, BODY_KINDS, RequestTab, SCRIPT_KINDS, SECTIONS, Section};
use crate::appearance::dp;
use crate::ui::{ReportUnlessGone as _, code_box, hint, labelled_input, scroll, section_tab, section_tab_strip, segmented};

impl RequestTab {
    fn section_title(&self, section: Section, cx: &App) -> (String, Option<usize>) {
        match section {
            Section::Params => ("Params".into(), Some(self.params.read(cx).items(cx).len())),
            Section::Body => ("Body".into(), None),
            Section::Headers => ("Headers".into(), Some(self.headers.read(cx).items(cx).len())),
            Section::Auth => {
                let label = AUTH_MODES.iter().find(|(mode, _)| *mode == self.auth_mode).map(|(_, label)| *label).unwrap_or("Auth");
                (format!("Auth · {label}"), None)
            }
            Section::Vars => ("Vars".into(), Some(self.vars.read(cx).items(cx).len())),
            Section::Scripts => {
                ("Scripts".into(), Some(self.scripts.iter().filter(|(_, editor)| !editor.read(cx).value().trim().is_empty()).count()))
            }
            Section::Docs => ("Docs".into(), None),
        }
    }

    pub(super) fn render_request(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tabs = SECTIONS.iter().enumerate().map(|(index, section)| {
            let (label, count) = self.section_title(*section, cx);
            let section = *section;
            section_tab(("request-section", index), label, count, section == self.section, cx).on_click(cx.listener(
                move |tab, _, _, cx| {
                    tab.section = section;
                    cx.notify();
                },
            ))
        });
        let tabs: Vec<_> = tabs.collect();
        let content: AnyElement = match self.section {
            Section::Params => scroll("params-scroll", self.params.clone().into_any_element()),
            Section::Headers => scroll("headers-scroll", self.headers.clone().into_any_element()),
            Section::Vars => v_flex()
                .size_full()
                .gap_2()
                .child(hint("Request variables win over folder, environment and collection ones.", cx))
                .child(scroll("vars-scroll", self.vars.clone().into_any_element()))
                .into_any_element(),
            Section::Body => self.render_body(cx),
            Section::Auth => self.render_auth(window, cx),
            Section::Scripts => self.render_scripts(cx),
            Section::Docs => code_box(Editor::new(&self.docs).bordered(false).h_full(), cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .child(section_tab_strip("request-sections", tabs, cx))
            .child(div().flex_1().min_h_0().px(dp(16.)).pt(dp(14.)).pb(dp(16.)).child(content))
            .into_any_element()
    }

    pub(super) fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let tab = cx.entity().downgrade();
        let kinds = BODY_KINDS.iter().map(|(kind, label)| (kind.to_string(), label.to_string())).collect();
        let picker = segmented(
            "body-kind",
            kinds,
            &self.body_kind,
            move |kind, window, cx| {
                tab.update(cx, |tab, cx| tab.set_body_kind(&kind, window, cx)).report_unless_gone("change the body type");
            },
            cx,
        );
        let content: AnyElement = match self.body_kind.as_str() {
            "none" => hint("This request sends no body.", cx).into_any_element(),
            "form-urlencoded" | "multipart-form" => scroll("fields-scroll", self.body_fields.clone().into_any_element()),
            "graphql" => v_flex()
                .size_full()
                .gap_2()
                .child(div().h(gpui::relative(0.62)).min_h_0().child(code_box(Editor::new(&self.body).bordered(false).h_full(), cx)))
                .child(hint("Variables", cx))
                .child(div().flex_1().min_h_0().child(code_box(Editor::new(&self.graphql_vars).bordered(false).h_full(), cx)))
                .into_any_element(),
            _ => code_box(Editor::new(&self.body).bordered(false).h_full(), cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .gap(dp(12.))
            .child(h_flex().min_w_0().child(picker))
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }

    pub(super) fn render_auth(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tab = cx.entity().downgrade();
        let modes = AUTH_MODES.iter().map(|(mode, label)| (mode.to_string(), label.to_string())).collect();
        let picker = segmented(
            "auth-mode",
            modes,
            &self.auth_mode,
            move |mode, window, cx| {
                tab.update(cx, |tab, cx| tab.set_auth_mode(&mode, window, cx)).report_unless_gone("change the auth");
            },
            cx,
        );
        let project = self.project.read(cx);
        let inherited = Plan::with_file(&project.collection, &self.path, self.build_file(cx), None)
            .map(|plan| plan.auth().mode().to_string())
            .unwrap_or_default();
        let fields = || {
            v_flex()
                .gap_2()
                .children(self.auth_inputs.iter().map(|(key, input)| labelled_input(key.rsplit('.').next().unwrap_or(key), input, cx)))
        };
        let body: AnyElement = match self.auth_mode.as_str() {
            "inherit" => hint(&format!("Uses the auth of its folder or the collection: {inherited}."), cx).into_any_element(),
            "none" => hint("Sends no credentials.", cx).into_any_element(),
            "oauth2" => fields()
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
                        .when_some(self.token_status.clone(), |el, status| el.child(hint(&status, cx))),
                )
                .into_any_element(),
            _ => fields().into_any_element(),
        };
        v_flex().gap(dp(14.)).child(h_flex().min_w_0().child(picker)).child(body).into_any_element()
    }

    pub(super) fn render_scripts(&self, cx: &mut Context<Self>) -> AnyElement {
        let tab = cx.entity().downgrade();
        let kinds = SCRIPT_KINDS
            .iter()
            .enumerate()
            .map(|(index, (_, label))| {
                let is_filled = !self.scripts[index].1.read(cx).value().trim().is_empty();
                (index, if is_filled { format!("{label} •") } else { label.to_string() })
            })
            .collect();
        let picker = segmented(
            "script-kind",
            kinds,
            &self.script_kind,
            move |index, _, cx| {
                tab.update(cx, |tab, cx| {
                    tab.script_kind = index;
                    cx.notify();
                })
                .report_unless_gone("pick a script");
            },
            cx,
        );
        let editor = &self.scripts[self.script_kind].1;
        v_flex()
            .size_full()
            .gap(dp(12.))
            .child(h_flex().min_w_0().child(picker))
            .child(div().flex_1().min_h_0().child(code_box(Editor::new(editor).bordered(false).h_full(), cx)))
            .child(hint("bru, req, res, test(), expect() and require('jsonwebtoken') work as in Bruno.", cx))
            .into_any_element()
    }
}
