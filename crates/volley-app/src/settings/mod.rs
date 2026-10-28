mod schemes;

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use gpui_component::switch::Switch;
use gpui_component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};

use crate::appearance::{self, dp, zoom};
use crate::prefs::{InterfaceFont, Prefs, ThemeChoice};
use crate::ui::{hint, section_title, segmented};

pub fn open(window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    window.open_dialog(cx, |dialog, _, cx| {
        dialog.title("Settings").w(dp(760.)).child(div().id("settings").max_h(dp(620.)).overflow_y_scroll().pr_2().child(content(cx)))
    });
}

fn content(cx: &App) -> AnyElement {
    let prefs = Prefs::load();
    v_flex()
        .gap_6()
        .pb_2()
        .child(appearance_section(&prefs, cx))
        .child(schemes::section(&prefs, cx))
        .child(size_section(&prefs, cx))
        .child(accessibility_section(&prefs, cx))
        .into_any_element()
}

fn row(title: &str, description: Option<&str>, control: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_4()
        .items_start()
        .justify_between()
        .child(
            v_flex()
                .w(dp(230.))
                .flex_none()
                .gap_0p5()
                .child(div().text_sm().child(title.to_string()))
                .children(description.map(|text| hint(text, cx))),
        )
        .child(div().flex_1().min_w_0().child(control))
}

fn group(title: &str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    v_flex()
        .gap_3()
        .child(section_title(title))
        .child(v_flex().gap_4().p_4().rounded_lg().border_1().border_color(cx.theme().border).bg(cx.theme().sidebar).children(rows))
        .into_any_element()
}

fn appearance_section(prefs: &Prefs, cx: &App) -> AnyElement {
    let themes = vec![
        (ThemeChoice::System, "System".to_string()),
        (ThemeChoice::Light, "Light".to_string()),
        (ThemeChoice::Dark, "Dark".to_string()),
    ];
    let fonts = vec![(InterfaceFont::Inter, "Inter".to_string()), (InterfaceFont::System, "System font".to_string())];
    group(
        "Appearance",
        vec![
            row(
                "Theme",
                Some("System follows the light or dark mode of your desktop."),
                segmented("theme", themes, &prefs.theme, |choice, _, cx| appearance::set_theme(choice, cx)),
                cx,
            )
            .into_any_element(),
            row(
                "Interface font",
                None,
                segmented("ui-font", fonts, &prefs.interface_font, |font, _, cx| appearance::set_interface_font(font, cx)),
                cx,
            )
            .into_any_element(),
        ],
        cx,
    )
}

fn size_section(prefs: &Prefs, cx: &App) -> AnyElement {
    let zoom_steps = zoom::ZOOM_STEPS.iter().map(|step| (*step, format!("{step}%"))).collect();
    let text_steps = zoom::TEXT_STEPS.iter().map(|(step, label)| (*step, label.to_string())).collect();
    let code_steps = zoom::CODE_FONT_STEPS.iter().map(|size| (*size, size.to_string())).collect();
    group(
        "Size",
        vec![
            row(
                "Interface zoom",
                Some("Everything at once, as a browser zooms a page. Ctrl+= and Ctrl+- change it, Ctrl+0 resets it."),
                segmented("zoom", zoom_steps, &prefs.interface_scale, |percent, _, cx| appearance::set_interface_scale(percent, cx)),
                cx,
            )
            .into_any_element(),
            row(
                "Text size",
                Some("Only the text and the controls around it; panels keep their size."),
                segmented("text-size", text_steps, &prefs.text_scale, |percent, _, cx| appearance::set_text_scale(percent, cx)),
                cx,
            )
            .into_any_element(),
            row(
                "Code font size",
                Some("Bodies, scripts and responses."),
                segmented("code-size", code_steps, &prefs.code_font_size, |size, _, cx| appearance::set_code_font_size(size, cx)),
                cx,
            )
            .into_any_element(),
        ],
        cx,
    )
}

fn accessibility_section(prefs: &Prefs, cx: &App) -> AnyElement {
    let switch =
        Switch::new("low-vision").checked(prefs.is_low_vision).on_click(|is_on: &bool, _, cx| appearance::set_low_vision(*is_on, cx));
    group(
        "Accessibility",
        vec![
            row(
                "Low vision mode",
                Some("Draws everything 25% larger, text in full black or white and borders that stand out. Works on top of the zoom."),
                switch,
                cx,
            )
            .into_any_element(),
        ],
        cx,
    )
}
