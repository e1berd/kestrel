use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};

use super::ProjectView;
use crate::appearance::dp;
use crate::ui::method_label;

impl ProjectView {
    pub(super) fn render_tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex().id("tabs").flex_none().h(dp(36.)).overflow_x_scroll().bg(theme.tab_bar).border_b_1().border_color(theme.border).children(
            self.tabs.iter().enumerate().map(|(index, tab)| {
                let tab = tab.read(cx);
                let is_active = index == self.active;
                h_flex()
                    .id(("tab", index))
                    .h_full()
                    .flex_none()
                    .max_w(dp(260.))
                    .px_3()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .cursor_pointer()
                    .border_r_1()
                    .border_color(theme.border)
                    .when(is_active, |el| {
                        el.bg(theme.tab_active).text_color(theme.tab_active_foreground).border_t_2().border_color(theme.primary)
                    })
                    .when(!is_active, |el| el.text_color(theme.tab_foreground).hover(|el| el.bg(theme.list_hover)))
                    .child(method_label(tab.method(), cx))
                    .child(div().min_w_0().truncate().child(tab.title()))
                    .child(
                        div()
                            .id(("close-tab", index))
                            .flex_none()
                            .w(dp(16.))
                            .flex()
                            .justify_center()
                            .child(if tab.dirty {
                                div().size(dp(7.)).rounded_full().bg(theme.primary).into_any_element()
                            } else {
                                Icon::new(IconName::Close).xsmall().text_color(theme.muted_foreground).into_any_element()
                            })
                            .on_click(cx.listener(move |view, _, window, cx| {
                                cx.stop_propagation();
                                view.close_tab(index, window, cx);
                            })),
                    )
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |view, _, window, cx| view.close_tab(index, window, cx)))
                    .on_click(cx.listener(move |view, _, _, cx| view.activate_tab(index, cx)))
            }),
        )
    }
}
