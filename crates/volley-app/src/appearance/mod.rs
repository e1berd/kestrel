mod expand;
pub mod palette;
pub mod zoom;

use std::borrow::Cow;

use gpui::{App, Global, KeyBinding, Window, actions, px};
use gpui_component::theme::{Theme, ThemeMode};
use volley_core::report::report_error;

use crate::prefs::{InterfaceFont, Prefs, ThemeChoice};
use palette::{ColorScheme, Palette};
pub use zoom::dp;

const FONTS: [&[u8]; 6] = [
    include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf"),
];

pub const INTERFACE_FONT: &str = "Inter";
pub const MONO_FONT: &str = "JetBrains Mono";

actions!(appearance, [ZoomIn, ZoomOut, ZoomReset]);

struct Schemes(Vec<ColorScheme>);
impl Global for Schemes {}

struct ActivePalette(Palette);
impl Global for ActivePalette {}

pub fn init(cx: &mut App) {
    if let Err(error) = cx.text_system().add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect()) {
        report_error("load the bundled fonts", error);
    }
    cx.set_global(Schemes(palette::load_all()));
    cx.bind_keys([
        KeyBinding::new(primary!("="), ZoomIn, None),
        KeyBinding::new(primary!("+"), ZoomIn, None),
        KeyBinding::new(primary!("-"), ZoomOut, None),
        KeyBinding::new(primary!("0"), ZoomReset, None),
    ]);
    cx.on_action(|_: &ZoomIn, cx| set_interface_scale(zoom::step_from(Prefs::load().interface_scale, true), cx));
    cx.on_action(|_: &ZoomOut, cx| set_interface_scale(zoom::step_from(Prefs::load().interface_scale, false), cx));
    cx.on_action(|_: &ZoomReset, cx| set_interface_scale(100, cx));
    apply(None, cx);
}

pub fn schemes(cx: &App) -> &[ColorScheme] {
    &cx.global::<Schemes>().0
}

pub fn reload_schemes(cx: &mut App) {
    cx.set_global(Schemes(palette::load_all()));
    apply(None, cx);
}

pub fn palette(cx: &App) -> Palette {
    cx.global::<ActivePalette>().0.clone()
}

pub fn apply(window: Option<&mut Window>, cx: &mut App) {
    let prefs = Prefs::load();
    match prefs.theme {
        ThemeChoice::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, window, cx),
        ThemeChoice::System => Theme::sync_system_appearance(window, cx),
    }
    let is_dark = Theme::global(cx).is_dark();
    let scheme = schemes(cx).iter().find(|s| s.id == prefs.scheme).or_else(|| schemes(cx).first()).cloned();
    let Some(scheme) = scheme else { return };
    let palette = if is_dark { scheme.dark } else { scheme.light };
    let boost = if prefs.is_low_vision { zoom::LOW_VISION_BOOST } else { 1. };
    zoom::set_zoom(prefs.interface_scale as f32 / 100. * boost);
    let text = prefs.text_scale as f32 / 100.;
    Theme::update(cx, |theme| {
        expand::apply_palette(&palette, &mut theme.colors, is_dark);
        if prefs.is_low_vision {
            expand::raise_contrast(&mut theme.colors, is_dark);
        }
        theme.font_family = match prefs.interface_font {
            InterfaceFont::Inter => INTERFACE_FONT.into(),
            InterfaceFont::System => ".SystemUIFont".into(),
        };
        theme.mono_font_family = MONO_FONT.into();
        theme.font_size = px(zoom::BASE_REM * zoom::zoom() * text);
        theme.mono_font_size = px(prefs.code_font_size as f32 * zoom::zoom());
        theme.radius = dp(6.);
        theme.radius_lg = dp(10.);
        theme.shadow = false;
        theme.highlight_theme = expand::editor_colors(&theme.highlight_theme, &palette);
    });
    cx.set_global(ActivePalette(palette));
    cx.refresh_windows();
}

pub fn set_theme(choice: ThemeChoice, cx: &mut App) {
    Prefs::update(|prefs| prefs.theme = choice);
    apply(None, cx);
}

pub fn set_scheme(id: &str, cx: &mut App) {
    Prefs::update(|prefs| prefs.scheme = id.to_string());
    apply(None, cx);
}

pub fn set_interface_scale(percent: u32, cx: &mut App) {
    Prefs::update(|prefs| prefs.interface_scale = percent);
    apply(None, cx);
}

pub fn set_text_scale(percent: u32, cx: &mut App) {
    Prefs::update(|prefs| prefs.text_scale = percent);
    apply(None, cx);
}

pub fn set_code_font_size(size: u32, cx: &mut App) {
    Prefs::update(|prefs| prefs.code_font_size = size);
    apply(None, cx);
}

pub fn set_low_vision(is_low_vision: bool, cx: &mut App) {
    Prefs::update(|prefs| prefs.is_low_vision = is_low_vision);
    apply(None, cx);
}

pub fn set_interface_font(font: InterfaceFont, cx: &mut App) {
    Prefs::update(|prefs| prefs.interface_font = font);
    apply(None, cx);
}

pub fn follow_system_appearance(window: &mut Window, cx: &mut App) {
    if Prefs::load().theme == ThemeChoice::System {
        apply(Some(window), cx);
    }
}
