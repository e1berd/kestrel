use gpui::{App, FontWeight, IntoElement, ParentElement as _, Styled as _, div, img, rems};
use gpui_component::{h_flex, v_flex};

use crate::appearance::{self, dp};
use crate::assets;
use crate::ui::keys;
use crate::workspace::shortcut;

const SHORTCUTS: [(&str, &str); 5] =
    [("Find a request or a command", "K"), ("Send the request", "Enter"), ("Save", "S"), ("New request", "N"), ("Settings", ",")];

pub fn render_empty_editor(cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(dp(6.))
        .child(img(assets::LOGO).size(dp(64.)).mb(dp(10.)))
        .child(div().text_size(rems(1.05)).font_weight(FontWeight::SEMIBOLD).child("Pick a request"))
        .child(div().text_size(rems(0.86)).text_color(palette.muted).child("Choose one on the left, or find it from anywhere."))
        .child(v_flex().w(dp(340.)).mt(dp(18.)).gap(dp(10.)).children(SHORTCUTS.iter().map(|(label, key)| {
            h_flex()
                .justify_between()
                .items_center()
                .text_size(rems(0.82))
                .text_color(palette.muted)
                .child(label.to_string())
                .child(keys(&shortcut(key), cx))
        })))
}
