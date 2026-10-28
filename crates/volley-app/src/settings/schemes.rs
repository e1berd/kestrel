use std::path::PathBuf;

use anyhow::Context as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::notification::Notification;
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, WindowExt as _, h_flex, v_flex};

use crate::appearance::palette::{ColorScheme, Palette, built_in_source, user_schemes_dir};
use crate::appearance::{self, dp};
use crate::prefs::Prefs;
use crate::ui::{hint, section_title};

pub fn section(prefs: &Prefs, cx: &App) -> AnyElement {
    let is_dark = cx.theme().is_dark();
    let cards = appearance::schemes(cx).iter().map(|scheme| scheme_card(scheme, scheme.id == prefs.scheme, is_dark, cx).into_any_element());
    v_flex()
        .gap_3()
        .child(
            h_flex().justify_between().items_center().child(section_title("Colour scheme")).child(
                h_flex()
                    .gap_2()
                    .child(Button::new("scheme-copy").small().outline().icon(IconName::Copy).label("Make my own").on_click(
                        |_, window, cx| match copy_current_scheme(cx) {
                            Ok(path) => {
                                window.push_notification(Notification::success(format!("Edit {} and press Reload.", path.display())), cx);
                                cx.reveal_path(&path);
                            }
                            Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
                        },
                    ))
                    .child(
                        Button::new("scheme-reload")
                            .small()
                            .ghost()
                            .icon(IconName::RefreshCw)
                            .label("Reload")
                            .on_click(|_, _, cx| appearance::reload_schemes(cx)),
                    ),
            ),
        )
        .child(h_flex().gap_3().flex_wrap().children(cards))
        .child(hint("A scheme is a small JSON file with a light and a dark palette. Your own live in ~/.config/kestrel/themes.", cx))
        .into_any_element()
}

fn scheme_card(scheme: &ColorScheme, is_selected: bool, is_dark: bool, cx: &App) -> impl IntoElement {
    let palette = if is_dark { &scheme.dark } else { &scheme.light };
    let id = scheme.id.clone();
    div()
        .id(gpui::ElementId::Name(format!("scheme-{}", scheme.id).into()))
        .w(dp(168.))
        .rounded_lg()
        .border_2()
        .border_color(if is_selected { cx.theme().primary } else { cx.theme().border })
        .cursor_pointer()
        .overflow_hidden()
        .child(preview(palette))
        .child(
            h_flex()
                .px_2()
                .py_1p5()
                .justify_between()
                .bg(cx.theme().background)
                .text_sm()
                .child(scheme.name.clone())
                .when(scheme.file.is_some(), |el| el.child(hint("yours", cx))),
        )
        .on_click(move |_, _, cx| appearance::set_scheme(&id, cx))
}

fn preview(palette: &Palette) -> impl IntoElement {
    let bar = |color: Hsla, width: f32| div().h(dp(6.)).w(dp(width)).rounded_full().bg(color);
    h_flex()
        .h(dp(78.))
        .bg(palette.background)
        .child(
            v_flex()
                .w(dp(46.))
                .h_full()
                .p_2()
                .gap_1p5()
                .bg(palette.surface)
                .child(bar(palette.methods.get, 26.))
                .child(bar(palette.methods.post, 30.))
                .child(bar(palette.muted, 22.)),
        )
        .child(
            v_flex()
                .flex_1()
                .h_full()
                .p_2()
                .gap_2()
                .child(h_flex().gap_1().child(bar(palette.text, 54.)).child(div().h(dp(12.)).w(dp(30.)).rounded_sm().bg(palette.accent)))
                .child(bar(palette.muted, 70.))
                .child(bar(palette.success, 40.)),
        )
}

fn copy_current_scheme(cx: &App) -> anyhow::Result<PathBuf> {
    let current = Prefs::load().scheme;
    let schemes = appearance::schemes(cx);
    let scheme = schemes.iter().find(|s| s.id == current).or(schemes.first()).context("there is no colour scheme to start from")?;
    let source = match &scheme.file {
        Some(file) => std::fs::read_to_string(file)?,
        None => built_in_source(&scheme.id).context("the built-in scheme is missing")?.to_string(),
    };
    let dir = user_schemes_dir().context("there is no configuration folder")?;
    std::fs::create_dir_all(&dir)?;
    let stem = scheme.id.trim_start_matches("user:");
    let path = (1..).map(|n| dir.join(format!("{stem}-{n}.json"))).find(|p| !p.exists()).context("no free file name")?;
    let renamed = source.replacen(&format!("\"name\": \"{}\"", scheme.name), &format!("\"name\": \"{} (mine)\"", scheme.name), 1);
    std::fs::write(&path, renamed)?;
    Ok(path)
}
