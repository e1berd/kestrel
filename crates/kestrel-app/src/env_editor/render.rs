use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};

use super::EnvEditor;
use crate::appearance::{dp, palette};

impl EnvEditor {
    fn render_environment_list(&self, names: &[String], active: Option<&str>, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let success = palette(cx).success;
        v_flex()
            .w(dp(210.))
            .h_full()
            .flex_none()
            .gap_2()
            .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child("ENVIRONMENTS"))
            .child(v_flex().id("env-list").flex_1().min_h_0().overflow_y_scroll().gap_1().children(names.iter().enumerate().map(
                |(index, name)| {
                    let in_use = active == Some(name.as_str());
                    h_flex()
                        .id(("env", index))
                        .gap_2()
                        .items_center()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .text_sm()
                        .when(index == self.selected, |el| el.bg(theme.list_active).font_weight(FontWeight::MEDIUM))
                        .hover(|el| el.bg(theme.list_hover))
                        .child(div().size(dp(8.)).flex_none().rounded_full().bg(if in_use { success } else { theme.border }))
                        .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                        .on_click(cx.listener(move |this, _, window, cx| this.select(index, window, cx)))
                },
            )))
            .child(
                v_flex().gap_2().pt_3().border_t_1().border_color(theme.border).child(Input::new(&self.new_name).small()).child(
                    Button::new("env-add")
                        .small()
                        .w_full()
                        .icon(IconName::Plus)
                        .label("Add environment")
                        .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                ),
            )
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .text_center()
            .child(Icon::new(IconName::Globe).size(dp(28.)).text_color(theme.muted_foreground))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("No environments yet"))
            .child(
                div()
                    .max_w(dp(320.))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("An environment is a set of variables, like a base URL, that you reference as {{name}}. Type a name on the left and press Enter."),
            )
    }

    fn render_variables_pane(&self, name: String, in_use: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let header_cell = |label: &'static str| div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(muted).child(label);
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(name)))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(if in_use {
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .text_sm()
                                    .text_color(palette(cx).success)
                                    .child(Icon::new(IconName::Check).small())
                                    .child("In use")
                                    .into_any_element()
                            } else {
                                Button::new("env-use")
                                    .small()
                                    .outline()
                                    .label("Use this environment")
                                    .on_click(cx.listener(|this, _, _, cx| this.use_selected(cx)))
                                    .into_any_element()
                            })
                            .child(
                                Button::new("env-delete")
                                    .small()
                                    .ghost()
                                    .icon(IconName::Delete)
                                    .tooltip("Delete environment")
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_delete(window, cx))),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .pb_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().w(dp(20.)))
                    .child(div().w(gpui::relative(0.35)).flex_none().child(header_cell("VARIABLE")))
                    .child(div().flex_1().child(header_cell("VALUE"))),
            )
            .child(div().id("env-scroll").flex_1().min_h_0().overflow_y_scroll().child(self.table.clone()))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div().flex_1().text_xs().text_color(muted).child("Secrets belong in .env (git ignores it): {{process.env.NAME}}"),
                    )
                    .child(
                        Button::new("env-save").primary().label("Save").on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

impl Render for EnvEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let names: Vec<String> = self.project.read(cx).collection.environments.iter().map(|environment| environment.name()).collect();
        let active = self.project.read(cx).env.clone();
        let selected_name = names.get(self.selected).cloned();
        let in_use = selected_name.is_some() && selected_name == active;
        let border = cx.theme().border;
        h_flex()
            .h(dp(460.))
            .gap_4()
            .items_stretch()
            .child(self.render_environment_list(&names, active.as_deref(), cx))
            .child(div().w_px().flex_none().bg(border))
            .child(match selected_name {
                Some(name) => self.render_variables_pane(name, in_use, cx).into_any_element(),
                None => self.render_empty_state(cx).into_any_element(),
            })
    }
}
