use std::path::PathBuf;

use anyhow::Context as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    div,
};
use gpui_component::notification::Notification;
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit_assets::IconName;

use super::appearance_section::swatch;
use crate::appearance::palette::{ColorScheme, built_in_source, user_schemes_dir};
use crate::appearance::{self, dp};
use crate::prefs::Prefs;
use crate::ui::hint;

pub fn section(prefs: &Prefs, cx: &App) -> AnyElement {
    let palette = appearance::palette(cx);
    let is_dark = cx.theme().is_dark();
    let pills = appearance::schemes(cx).iter().map(|scheme| scheme_pill(scheme, scheme.id == prefs.scheme, is_dark, cx).into_any_element());
    let action = |id: &'static str, icon: IconName, label: &'static str| {
        h_flex()
            .id(id)
            .h(dp(40.))
            .px(dp(14.))
            .gap(dp(8.))
            .items_center()
            .rounded(dp(10.))
            .border_1()
            .border_dashed()
            .border_color(palette.border)
            .cursor_pointer()
            .text_color(palette.muted)
            .hover(|el| el.text_color(palette.text).border_color(palette.muted))
            .child(Icon::new(icon).small())
            .child(label)
    };
    v_flex()
        .gap(dp(10.))
        .child(div().font_weight(FontWeight::MEDIUM).child("Colour scheme"))
        .child(
            h_flex()
                .flex_wrap()
                .gap(dp(10.))
                .children(pills)
                .child(action("scheme-copy", IconName::Plus, "Make my own").on_click(|_, window, cx| match copy_current_scheme(cx) {
                    Ok(path) => {
                        window.push_notification(Notification::success(format!("Edit {} and press Reload.", path.display())), cx);
                        cx.reveal_path(&path);
                    }
                    Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
                }))
                .child(action("scheme-reload", IconName::RefreshCw, "Reload").on_click(|_, _, cx| appearance::reload_schemes(cx))),
        )
        .child(hint("A scheme is a small JSON file with a light and a dark palette. Your own live in ~/.config/kestrel/themes.", cx))
        .into_any_element()
}

fn scheme_pill(scheme: &ColorScheme, is_selected: bool, is_dark: bool, cx: &App) -> impl IntoElement {
    let current = appearance::palette(cx);
    let palette = if is_dark { &scheme.dark } else { &scheme.light };
    let id = scheme.id.clone();
    h_flex()
        .id(gpui::ElementId::Name(format!("scheme-{}", scheme.id).into()))
        .h(dp(40.))
        .pl(dp(10.))
        .pr(dp(14.))
        .gap(dp(10.))
        .items_center()
        .rounded(dp(10.))
        .border_1()
        .cursor_pointer()
        .font_weight(FontWeight::MEDIUM)
        .border_color(if is_selected { current.accent } else { current.border })
        .when(is_selected, |el| el.bg(current.raised))
        .when(!is_selected, |el| el.hover(|el| el.bg(current.hover)))
        .child(h_flex().gap(dp(4.)).child(swatch(palette.accent)).child(swatch(palette.methods.get)).child(swatch(palette.background)))
        .child(scheme.name.clone())
        .when(scheme.file.is_some(), |el| el.child(hint("yours", cx)))
        .on_click(move |_, _, cx| appearance::set_scheme(&id, cx))
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
