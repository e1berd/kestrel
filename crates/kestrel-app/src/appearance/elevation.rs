use gpui::{App, BoxShadow, Hsla, Pixels, point};
use gpui_component::ActiveTheme as _;

use super::dp;

fn shadow(cx: &App, light: f32, dark: f32, offset_y: f32, blur: f32, spread: f32) -> BoxShadow {
    let alpha = if cx.theme().is_dark() { dark } else { light };
    BoxShadow {
        color: Hsla { a: alpha, ..gpui::black() },
        offset: point(Pixels::ZERO, dp(offset_y)),
        blur_radius: dp(blur),
        spread_radius: dp(spread),
        inset: false,
    }
}

pub fn card(cx: &App) -> Vec<BoxShadow> {
    vec![shadow(cx, 0.04, 0.22, 1., 2., 0.), shadow(cx, 0.05, 0.28, 8., 24., -6.)]
}

pub fn lifted(cx: &App) -> Vec<BoxShadow> {
    vec![shadow(cx, 0.08, 0.32, 1., 3., 0.)]
}
