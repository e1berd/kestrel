mod appearance_section;
mod schemes;
mod size_section;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use gpui_component::switch::Switch;
use gpui_component::{WindowExt as _, h_flex, v_flex};

use crate::appearance::{self, MONO_FONT, dp};
use crate::prefs::Prefs;
use crate::ui::hint;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Appearance,
    Size,
    Accessibility,
    Shortcuts,
}

const PAGES: [(Page, &str); 4] = [
    (Page::Appearance, "Appearance"),
    (Page::Size, "Zoom and sizes"),
    (Page::Accessibility, "Accessibility"),
    (Page::Shortcuts, "Shortcuts"),
];

const SHORTCUTS: [(&str, &str); 13] = [
    ("Send the request", "Ctrl Enter"),
    ("Save", "Ctrl S"),
    ("Find a request or a command", "Ctrl K"),
    ("New request", "Ctrl N"),
    ("Close the tab", "Ctrl W"),
    ("Next and previous tab", "Ctrl Tab · Ctrl Shift Tab"),
    ("Next and previous project", "Ctrl Shift ] · Ctrl Shift ["),
    ("Close the project", "Ctrl Shift W"),
    ("Open a collection", "Ctrl O"),
    ("Reload from disk", "Ctrl R"),
    ("Settings", "Ctrl ,"),
    ("Zoom in and out", "Ctrl = · Ctrl -"),
    ("Reset the zoom", "Ctrl 0"),
];

pub struct SettingsView {
    page: Page,
}

pub fn open(window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let view = cx.new(|_| SettingsView { page: Page::Appearance });
    window.open_dialog(cx, move |dialog, _, _| dialog.w(dp(940.)).p_0().child(view.clone()));
}

impl SettingsView {
    fn render_nav(&self, cx: &mut Context<Self>) -> gpui::Div {
        let palette = appearance::palette(cx);
        v_flex()
            .w(dp(220.))
            .flex_none()
            .p(dp(12.))
            .gap(dp(2.))
            .bg(palette.surface)
            .border_r_1()
            .border_color(palette.border_soft)
            .child(div().px(dp(10.)).pt(dp(8.)).pb(dp(12.)).text_size(rems(1.)).font_weight(FontWeight::SEMIBOLD).child("Settings"))
            .children(PAGES.iter().map(|(page, label)| {
                let page = *page;
                let is_active = page == self.page;
                div()
                    .id(gpui::ElementId::Name(format!("settings-{label}").into()))
                    .px(dp(10.))
                    .py(dp(8.))
                    .rounded(dp(8.))
                    .cursor_pointer()
                    .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                    .text_color(if is_active { palette.text } else { palette.muted })
                    .when(is_active, |el| el.bg(palette.raised))
                    .when(!is_active, |el| el.hover(|el| el.bg(palette.hover)))
                    .child(*label)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.page = page;
                        cx.notify();
                    }))
            }))
            .child(div().flex_1())
            .child(div().px(dp(10.)).child(hint(concat!("Kestrel ", env!("CARGO_PKG_VERSION"), " · MIT / Apache-2.0"), cx)))
    }

    fn render_page(&self, cx: &App) -> AnyElement {
        let prefs = Prefs::load();
        match self.page {
            Page::Appearance => appearance_section::render(&prefs, cx),
            Page::Size => size_section::render(&prefs, cx),
            Page::Accessibility => accessibility(&prefs, cx),
            Page::Shortcuts => shortcuts(cx),
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().h(dp(600.)).items_start().rounded(dp(12.)).overflow_hidden().child(self.render_nav(cx).h_full()).child(
            div().id("settings-page").flex_1().min_w_0().h_full().overflow_y_scroll().px(dp(28.)).py(dp(24.)).child(self.render_page(cx)),
        )
    }
}

pub(super) fn page_header(title: &str, description: &str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap(dp(4.))
        .child(div().text_size(rems(1.15)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
        .child(div().text_color(appearance::palette(cx).muted).child(description.to_string()))
}

pub(super) fn card(rows: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    let mut card = v_flex().p(dp(18.)).gap(dp(14.)).rounded(dp(12.)).bg(palette.surface).border_1().border_color(palette.border_soft);
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.child(div().h(dp(1.)).bg(palette.border_soft));
        }
        card = card.child(row);
    }
    card
}

pub(super) fn row(title: &str, description: Option<&str>, control: impl IntoElement, cx: &App) -> AnyElement {
    h_flex()
        .gap(dp(16.))
        .items_center()
        .flex_wrap()
        .child(
            v_flex()
                .flex_1()
                .min_w(dp(200.))
                .gap(dp(2.))
                .child(div().font_weight(FontWeight::MEDIUM).child(title.to_string()))
                .children(description.map(|text| hint(text, cx))),
        )
        .child(control)
        .into_any_element()
}

fn accessibility(prefs: &Prefs, cx: &App) -> AnyElement {
    let switch =
        Switch::new("low-vision").checked(prefs.is_low_vision).on_click(|is_on: &bool, _, cx| appearance::set_low_vision(*is_on, cx));
    v_flex()
        .gap(dp(16.))
        .child(page_header("Accessibility", "For people who find the default text hard to read.", cx))
        .child(card(
            vec![row(
                "Low vision mode",
                Some("Everything 25% larger, text in pure white or black, borders that stand out. Works on top of the zoom."),
                switch,
                cx,
            )],
            cx,
        ))
        .into_any_element()
}

fn shortcuts(cx: &App) -> AnyElement {
    let palette = appearance::palette(cx);
    let rows = SHORTCUTS
        .iter()
        .map(|(action, keys)| {
            h_flex()
                .justify_between()
                .child(action.to_string())
                .child(
                    div()
                        .px(dp(8.))
                        .py(dp(2.))
                        .rounded(dp(6.))
                        .bg(palette.raised)
                        .font_family(MONO_FONT)
                        .text_size(rems(0.76))
                        .text_color(palette.muted)
                        .child(if cfg!(target_os = "macos") { keys.replace("Ctrl", "⌘") } else { keys.to_string() }),
                )
                .into_any_element()
        })
        .collect();
    v_flex().gap(dp(16.)).child(page_header("Shortcuts", "On macOS, Cmd instead of Ctrl.", cx)).child(card(rows, cx)).into_any_element()
}
