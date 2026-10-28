use gpui::prelude::FluentBuilder as _;
use gpui::{Context, FontWeight, IntoElement, ParentElement as _, Styled, div, rems};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _, h_flex, v_flex};

use super::{METHODS, RequestTab};
use crate::ui::ReportUnlessGone as _;
use crate::ui::*;

impl RequestTab {
    pub(super) fn render_url_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let method = self.method.clone();
        let sending = self.sending.is_some();
        let color = method_color(&method, cx);
        let theme = cx.theme();
        v_flex()
            .gap_1()
            .p_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("method")
                            .small()
                            .outline()
                            .child(div().w(rems(3.6)).text_color(color).font_weight(FontWeight::BOLD).child(method.clone()))
                            .dropdown_caret(true)
                            .dropdown_menu({
                                let tab = cx.entity().downgrade();
                                move |menu, _, _| {
                                    METHODS.iter().fold(menu, |menu, m| {
                                        let tab = tab.clone();
                                        menu.item(PopupMenuItem::new(*m).checked(*m == method).on_click(move |_, _, cx| {
                                            tab.update(cx, |tab, cx| tab.set_method(m, cx)).report_unless_gone("change the method");
                                        }))
                                    })
                                }
                            }),
                    )
                    .child(div().flex_1().min_w_0().child(Input::new(&self.url).small()))
                    .child(if sending {
                        Button::new("cancel").small().label("Cancel").on_click(cx.listener(|tab, _, _, cx| tab.cancel(cx)))
                    } else {
                        Button::new("send")
                            .small()
                            .primary()
                            .icon(IconName::Play)
                            .label("Send")
                            .tooltip("Send (Ctrl+Enter)")
                            .on_click(cx.listener(|tab, _, window, cx| tab.send(window, cx)))
                    })
                    .child(
                        Button::new("save")
                            .small()
                            .ghost()
                            .label("Save")
                            .tooltip("Save (Ctrl+S)")
                            .disabled(!self.dirty)
                            .on_click(cx.listener(|tab, _, window, cx| tab.save(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(div().truncate().child(self.preview.clone()))
                    .when(!self.missing.is_empty(), |el| {
                        el.child(div().flex_none().text_color(theme.warning).child(format!("not set: {}", self.missing.join(", "))))
                    }),
            )
            .when(self.changed_on_disk, |el| {
                el.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_xs()
                        .text_color(theme.warning)
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
                        }))),
                )
            })
    }
}
