use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, rems,
};
use gpui_component::{Icon, IconName, Sizable as _, h_flex};

use super::ProjectView;
use crate::appearance::{self, dp};
use crate::ui::method_label;

impl ProjectView {
    pub(super) fn render_tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let shadow = appearance::elevation::lifted(cx);
        h_flex()
            .id("tabs")
            .flex_none()
            .px(dp(8.))
            .py(dp(7.))
            .gap(dp(4.))
            .border_b_1()
            .border_color(palette.border_soft)
            .bg(palette.code)
            .overflow_x_scroll()
            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                let tab = tab.read(cx);
                let is_active = index == self.active;
                h_flex()
                    .id(("tab", index))
                    .flex_none()
                    .h(rems(2.1))
                    .max_w(dp(260.))
                    .pl(dp(12.))
                    .pr(dp(6.))
                    .gap(dp(8.))
                    .items_center()
                    .rounded(dp(9.))
                    .border_1()
                    .cursor_pointer()
                    .text_size(rems(0.84))
                    .when(is_active, |el| {
                        el.bg(palette.raised).border_color(palette.border_soft).text_color(palette.text).shadow(shadow.clone())
                    })
                    .when(!is_active, |el| {
                        el.border_color(gpui::transparent_black()).text_color(palette.muted).hover(|el| el.bg(palette.hover))
                    })
                    .child(method_label(tab.method(), cx))
                    .child(div().min_w_0().truncate().child(tab.title()))
                    .child(
                        div()
                            .id(("close-tab", index))
                            .flex_none()
                            .size(dp(18.))
                            .rounded(dp(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|el| el.bg(palette.hover))
                            .child(if tab.dirty {
                                div().size(dp(7.)).rounded_full().bg(palette.accent).into_any_element()
                            } else {
                                Icon::new(IconName::Close).xsmall().text_color(palette.faint).into_any_element()
                            })
                            .on_click(cx.listener(move |view, _, window, cx| {
                                cx.stop_propagation();
                                view.close_tab(index, window, cx);
                            })),
                    )
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |view, _, window, cx| view.close_tab(index, window, cx)))
                    .on_click(cx.listener(move |view, _, _, cx| view.activate_tab(index, cx)))
            }))
    }
}
