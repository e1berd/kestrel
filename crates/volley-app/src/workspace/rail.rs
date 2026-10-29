use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, Theme, v_flex};

use super::Workspace;
use crate::appearance::{dp, palette::mix};
use crate::ui::ReportUnlessGone as _;

impl Workspace {
    pub(super) fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("project-rail")
            .w(dp(52.))
            .h_full()
            .flex_none()
            .pt(dp(8.))
            .gap(dp(8.))
            .items_center()
            .children((0..self.views.len()).map(|index| self.render_project_badge(index, cx)))
            .child(self.render_open_menu(cx))
    }

    fn render_project_badge(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let view = self.views[index].read(cx);
        let name = view.name(cx);
        let root = view.root(cx);
        let has_unsaved = view.has_unsaved(cx);
        let is_active = index == self.active;
        let theme = cx.theme();
        let tint = badge_tint(&name, theme);
        let workspace = cx.entity().downgrade();
        let tooltip = format!("{name}\n{}", root.display());
        let palette = crate::appearance::palette(cx);
        let monogram = div()
            .id(("project", index))
            .relative()
            .size(dp(34.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(dp(10.))
            .border_1()
            .cursor_pointer()
            .text_size(gpui::rems(0.78))
            .font_weight(FontWeight::SEMIBOLD)
            .when(is_active, |el| {
                el.bg(palette.raised).border_color(palette.border).text_color(palette.text).shadow(crate::appearance::elevation::lifted(cx))
            })
            .when(!is_active, |el| {
                el.bg(tint).border_color(gpui::transparent_black()).text_color(palette.muted).hover(|el| el.text_color(palette.text))
            })
            .child(initials(&name))
            .when(has_unsaved, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(dp(-2.))
                        .right(dp(-2.))
                        .size(dp(9.))
                        .rounded_full()
                        .bg(theme.warning)
                        .border_2()
                        .border_color(theme.title_bar),
                )
            })
            .on_click(cx.listener(move |workspace, _, window, cx| workspace.activate(index, window, cx)))
            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx))
            .context_menu(move |menu, _, _| {
                let close = workspace.clone();
                let root = root.clone();
                menu.item(PopupMenuItem::new("Close project").on_click(move |_, window, cx| {
                    close.update(cx, |workspace, cx| workspace.close_project(index, window, cx)).report_unless_gone("close a project");
                }))
                .item(PopupMenuItem::new("Show in file manager").on_click(move |_, _, cx| cx.reveal_path(&root)))
            });
        div()
            .relative()
            .w_full()
            .flex()
            .justify_center()
            .when(is_active, |el| el.child(div().absolute().left_0().top(dp(8.)).w(dp(3.)).h(dp(18.)).rounded_r(dp(3.)).bg(palette.accent)))
            .child(monogram)
            .into_any_element()
    }

    fn render_open_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = cx.entity().downgrade();
        Button::new("open-project").small().ghost().icon(IconName::Plus).tooltip("Open or make a collection").dropdown_menu(
            move |menu, _, _| {
                let item = |label: &'static str, action: fn(&mut Workspace, &mut gpui::Window, &mut Context<Workspace>)| {
                    let workspace = workspace.clone();
                    PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        workspace.update(cx, |workspace, cx| action(workspace, window, cx)).report_unless_gone("open a collection");
                    })
                };
                menu.item(item("Open collection…", Workspace::prompt_open))
                    .item(item("New collection…", Workspace::prompt_new_collection))
                    .item(item("Import collection…", Workspace::prompt_import))
                    .item(item("Export collection…", Workspace::prompt_export))
            },
        )
    }
}

fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let letters: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect(),
        [first, second, ..] => first.chars().take(1).chain(second.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

fn badge_tint(name: &str, theme: &Theme) -> Hsla {
    let hue = name.bytes().fold(7u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte as u32)) % 360;
    let hue_color = Hsla { h: hue as f32 / 360., s: 0.45, l: if theme.is_dark() { 0.45 } else { 0.6 }, a: 1. };
    mix(theme.title_bar, hue_color, 0.18)
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_take_the_first_letters_of_two_words() {
        assert_eq!(initials("EvixDesk API"), "EA");
        assert_eq!(initials("httpbin"), "HT");
        assert_eq!(initials("my-shop backend"), "MS");
    }
}
