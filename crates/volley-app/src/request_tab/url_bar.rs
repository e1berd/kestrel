use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;

use super::{METHODS, RequestTab};
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{ReportUnlessGone as _, method_color};

impl RequestTab {
    pub(super) fn render_url_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let is_sending = self.sending.is_some();
        v_flex()
            .px(dp(16.))
            .pt(dp(14.))
            .pb(dp(12.))
            .gap(dp(8.))
            .child(
                h_flex()
                    .gap(dp(8.))
                    .items_center()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h(rems(2.8))
                            .p(dp(4.))
                            .items_center()
                            .rounded(dp(12.))
                            .border_1()
                            .border_color(palette.border)
                            .bg(palette.raised)
                            .shadow(appearance::elevation::lifted(cx))
                            .child(self.render_method_picker(cx))
                            .child(
                                div().flex_1().min_w_0().px(dp(4.)).font_family(MONO_FONT).child(Input::new(&self.url).appearance(false)),
                            )
                            .child(self.render_send_button(is_sending, cx)),
                    )
                    .child(self.render_save_button(cx)),
            )
            .child(
                h_flex()
                    .pl(dp(6.))
                    .gap(dp(8.))
                    .items_center()
                    .font_family(MONO_FONT)
                    .text_size(rems(0.74))
                    .text_color(palette.faint)
                    .child(Icon::new(IconName::ArrowRight).xsmall())
                    .child(div().min_w_0().truncate().child(self.preview.clone()))
                    .when(!self.missing.is_empty(), |el| {
                        el.child(div().flex_none().text_color(palette.warning).child(format!("not set: {}", self.missing.join(", "))))
                    }),
            )
            .when(self.changed_on_disk, |el| el.child(self.render_disk_conflict(cx)))
    }

    fn render_save_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        h_flex()
            .id("save")
            .h(rems(2.8))
            .px(dp(14.))
            .gap(dp(7.))
            .items_center()
            .rounded(dp(12.))
            .cursor_pointer()
            .font_weight(FontWeight::MEDIUM)
            .text_color(if self.dirty { palette.text } else { palette.faint })
            .hover(|el| el.bg(palette.hover).text_color(palette.text))
            .when(self.dirty, |el| el.child(div().size(dp(6.)).rounded_full().bg(palette.accent)))
            .child("Save")
            .on_click(cx.listener(|tab, _, window, cx| tab.save(window, cx)))
    }

    fn render_method_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let method = self.method.clone();
        let tab = cx.entity().downgrade();
        let label = h_flex()
            .gap(dp(6.))
            .items_center()
            .font_family(MONO_FONT)
            .text_size(rems(0.78))
            .font_weight(FontWeight::BOLD)
            .text_color(method_color(&method, cx))
            .child(method.clone())
            .child(Icon::new(gpui_component::IconName::ChevronDown).xsmall().text_color(palette.faint));
        div().h_full().flex().items_center().child(
            Button::new("method").ghost().h_full().rounded(dp(8.)).pl(dp(10.)).pr(dp(8.)).child(label).dropdown_menu(move |menu, _, _| {
                METHODS.iter().fold(menu, |menu, candidate| {
                    let tab = tab.clone();
                    menu.item(PopupMenuItem::new(*candidate).checked(*candidate == method).on_click(move |_, _, cx| {
                        tab.update(cx, |tab, cx| tab.set_method(candidate, cx)).report_unless_gone("change the method");
                    }))
                })
            }),
        )
    }

    fn render_send_button(&self, is_sending: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let label = if is_sending { "Cancel" } else { "Send" };
        h_flex()
            .id("send")
            .flex_none()
            .h_full()
            .px(dp(16.))
            .gap(dp(8.))
            .items_center()
            .rounded(dp(9.))
            .cursor_pointer()
            .font_weight(FontWeight::SEMIBOLD)
            .when(is_sending, |el| el.bg(palette.hover).text_color(palette.text))
            .when(!is_sending, |el| el.bg(palette.accent).text_color(palette.accent_text).hover(|el| el.opacity(0.9)))
            .child(Icon::new(if is_sending { IconName::CircleX } else { IconName::Send }).small())
            .child(label)
            .on_click(cx.listener(move |tab, _, window, cx| if is_sending { tab.cancel(cx) } else { tab.send(window, cx) }))
    }

    fn render_disk_conflict(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        h_flex()
            .gap_2()
            .items_center()
            .text_xs()
            .text_color(palette.warning)
            .child("The file changed on disk while you were editing it.")
            .child(
                Button::new("take-disk")
                    .xsmall()
                    .ghost()
                    .label("Use the file")
                    .on_click(cx.listener(|tab, _, window, cx| tab.revert(window, cx))),
            )
            .child(Button::new("keep-mine").xsmall().ghost().label("Keep mine").on_click(cx.listener(|tab, _, _, cx| {
                tab.changed_on_disk = false;
                cx.notify();
            })))
    }
}
