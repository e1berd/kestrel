use std::sync::Arc;

use gpui::Hsla;
use gpui_component::highlighter::HighlightTheme;
use gpui_component::theme::ThemeColor;

use super::palette::{Palette, mix};

pub fn apply_palette(palette: &Palette, colors: &mut ThemeColor, is_dark: bool) {
    let ink = if is_dark { gpui::white() } else { gpui::black() };
    let stronger = |color: Hsla, amount: f32| mix(color, ink, amount);
    let transparent = Hsla { a: 0., ..palette.background };

    colors.background = palette.background;
    colors.foreground = palette.text;
    colors.border = palette.border;
    colors.input = palette.border;
    colors.ring = palette.accent;
    colors.caret = palette.accent;
    colors.selection = palette.selection;
    colors.muted = palette.hover;
    colors.muted_foreground = palette.muted;
    colors.accent = palette.hover;
    colors.accent_foreground = palette.text;
    colors.popover = palette.raised;
    colors.popover_foreground = palette.text;
    colors.overlay = Hsla { a: 0.45, ..gpui::black() };
    colors.window_border = palette.border;
    colors.skeleton = palette.hover;
    colors.drag_border = palette.accent;
    colors.drop_target = Hsla { a: 0.12, ..palette.accent };

    colors.primary = palette.accent;
    colors.primary_hover = stronger(palette.accent, 0.08);
    colors.primary_active = stronger(palette.accent, 0.16);
    colors.primary_foreground = palette.accent_text;
    colors.secondary = palette.hover;
    colors.secondary_hover = stronger(palette.hover, 0.05);
    colors.secondary_active = stronger(palette.hover, 0.1);
    colors.secondary_foreground = palette.text;

    colors.button = palette.raised;
    colors.button_hover = palette.hover;
    colors.button_active = stronger(palette.hover, 0.06);
    colors.button_foreground = palette.text;
    colors.button_primary = palette.accent;
    colors.button_primary_hover = colors.primary_hover;
    colors.button_primary_active = colors.primary_active;
    colors.button_primary_foreground = palette.accent_text;
    colors.button_secondary = palette.hover;
    colors.button_secondary_hover = colors.secondary_hover;
    colors.button_secondary_active = colors.secondary_active;
    colors.button_secondary_foreground = palette.text;
    for (fill, hover, active, foreground, base) in [
        (
            &mut colors.button_danger,
            &mut colors.button_danger_hover,
            &mut colors.button_danger_active,
            &mut colors.button_danger_foreground,
            palette.danger,
        ),
        (
            &mut colors.button_success,
            &mut colors.button_success_hover,
            &mut colors.button_success_active,
            &mut colors.button_success_foreground,
            palette.success,
        ),
        (
            &mut colors.button_warning,
            &mut colors.button_warning_hover,
            &mut colors.button_warning_active,
            &mut colors.button_warning_foreground,
            palette.warning,
        ),
        (
            &mut colors.button_info,
            &mut colors.button_info_hover,
            &mut colors.button_info_active,
            &mut colors.button_info_foreground,
            palette.info,
        ),
    ] {
        *fill = base;
        *hover = stronger(base, 0.08);
        *active = stronger(base, 0.16);
        *foreground = palette.background;
    }

    for (fill, hover, active, foreground, base) in [
        (&mut colors.danger, &mut colors.danger_hover, &mut colors.danger_active, &mut colors.danger_foreground, palette.danger),
        (&mut colors.success, &mut colors.success_hover, &mut colors.success_active, &mut colors.success_foreground, palette.success),
        (&mut colors.warning, &mut colors.warning_hover, &mut colors.warning_active, &mut colors.warning_foreground, palette.warning),
        (&mut colors.info, &mut colors.info_hover, &mut colors.info_active, &mut colors.info_foreground, palette.info),
    ] {
        *fill = base;
        *hover = stronger(base, 0.08);
        *active = stronger(base, 0.16);
        *foreground = palette.background;
    }

    colors.link = palette.accent;
    colors.link_hover = colors.primary_hover;
    colors.link_active = colors.primary_active;

    colors.list = palette.background;
    colors.list_hover = palette.hover;
    colors.list_active = Hsla { a: 0.16, ..palette.accent };
    colors.list_active_border = palette.accent;
    colors.list_even = mix(palette.background, palette.surface, 0.5);
    colors.list_head = palette.surface;

    colors.table = palette.background;
    colors.table_hover = palette.hover;
    colors.table_active = colors.list_active;
    colors.table_active_border = palette.accent;
    colors.table_even = colors.list_even;
    colors.table_head = palette.surface;
    colors.table_head_foreground = palette.muted;
    colors.table_foot = palette.surface;
    colors.table_foot_foreground = palette.muted;
    colors.table_row_border = palette.border;

    colors.sidebar = palette.surface;
    colors.sidebar_foreground = palette.text;
    colors.sidebar_border = palette.border;
    colors.sidebar_accent = palette.hover;
    colors.sidebar_accent_foreground = palette.text;
    colors.sidebar_primary = palette.accent;
    colors.sidebar_primary_foreground = palette.accent_text;

    colors.title_bar = palette.surface;
    colors.title_bar_border = palette.border;
    colors.status_bar = palette.surface;
    colors.status_bar_border = palette.border;

    colors.tab_bar = palette.surface;
    colors.tab_bar_segmented = palette.hover;
    colors.tab = transparent;
    colors.tab_active = palette.background;
    colors.tab_active_foreground = palette.text;
    colors.tab_foreground = palette.muted;

    colors.scrollbar = transparent;
    colors.scrollbar_thumb = Hsla { a: 0.35, ..palette.muted };
    colors.scrollbar_thumb_hover = Hsla { a: 0.6, ..palette.muted };

    colors.switch = palette.border;
    colors.switch_thumb = palette.raised;
    colors.slider_bar = palette.accent;
    colors.slider_thumb = palette.raised;
    colors.progress_bar = palette.accent;

    colors.accordion = palette.background;
    colors.group_box = palette.surface;
    colors.group_box_foreground = palette.text;
    colors.description_list_label = palette.surface;
    colors.description_list_label_foreground = palette.muted;

    colors.red = palette.danger;
    colors.green = palette.success;
    colors.yellow = palette.warning;
    colors.blue = palette.info;
    colors.magenta = palette.methods.patch;
    colors.cyan = palette.methods.other;
    for (light, base) in [
        (&mut colors.red_light, palette.danger),
        (&mut colors.green_light, palette.success),
        (&mut colors.yellow_light, palette.warning),
        (&mut colors.blue_light, palette.info),
        (&mut colors.magenta_light, palette.methods.patch),
        (&mut colors.cyan_light, palette.methods.other),
    ] {
        *light = Hsla { a: 0.16, ..base };
    }
    colors.chart_1 = palette.accent;
    colors.chart_2 = palette.info;
    colors.chart_3 = palette.success;
    colors.chart_4 = palette.warning;
    colors.chart_5 = palette.methods.patch;
    colors.chart_bullish = palette.success;
    colors.chart_bearish = palette.danger;
    colors.chart_grid = Hsla { a: 0.5, ..palette.border };
}

pub fn raise_contrast(colors: &mut ThemeColor, is_dark: bool) {
    let ink = if is_dark { gpui::white() } else { gpui::black() };
    for text in [
        &mut colors.foreground,
        &mut colors.popover_foreground,
        &mut colors.secondary_foreground,
        &mut colors.accent_foreground,
        &mut colors.sidebar_foreground,
        &mut colors.tab_active_foreground,
        &mut colors.button_foreground,
    ] {
        *text = ink;
    }
    colors.muted_foreground = Hsla { a: 0.85, ..ink };
    colors.tab_foreground = Hsla { a: 0.85, ..ink };
    colors.border = Hsla { a: 0.6, ..ink };
    colors.sidebar_border = colors.border;
    colors.title_bar_border = colors.border;
    colors.input = Hsla { a: 0.75, ..ink };
}

pub fn editor_colors(base: &HighlightTheme, palette: &Palette) -> Arc<HighlightTheme> {
    let mut theme = base.clone();
    theme.style.editor_background = Some(palette.background);
    theme.style.editor_gutter_background = Some(palette.background);
    theme.style.editor_foreground = Some(palette.text);
    theme.style.editor_active_line = Some(Hsla { a: 0.5, ..palette.hover });
    theme.style.editor_line_number = Some(Hsla { a: 0.7, ..palette.muted });
    theme.style.editor_active_line_number = Some(palette.text);
    Arc::new(theme)
}
