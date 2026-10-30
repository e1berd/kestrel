use gpui::{Context, IntoElement, ParentElement as _, Styled as _, div, rems};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Sizable as _, h_flex};

use super::{RequestTab, Section};
use crate::appearance::{self, dp};

impl RequestTab {
    pub(super) fn render_missing_path_params(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let names = self.missing_path_params.iter().map(|name| format!(":{name}")).collect::<Vec<_>>().join(", ");
        h_flex()
            .gap(dp(8.))
            .items_center()
            .flex_wrap()
            .text_size(rems(0.78))
            .text_color(palette.warning)
            .child(div().child(format!("Set path parameters before sending: {names}")))
            .child(Button::new("edit-path-params").small().ghost().label("Edit in Params").on_click(cx.listener(|tab, _, window, cx| {
                tab.sync_path_params(window, cx);
                tab.section = Section::Params;
                tab.changed(cx);
            })))
    }
}
