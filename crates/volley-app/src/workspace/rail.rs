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
use crate::settings;
use crate::ui::ReportUnlessGone as _;

impl Workspace {
    pub(super) fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("project-rail")
            .w(dp(58.))
            .h_full()
            .flex_none()
            .py_2()
            .gap_2()
            .items_center()
            .bg(theme.title_bar)
            .border_r_1()
            .border_color(theme.border)
            .children((0..self.views.len()).map(|index| self.render_project_badge(index, cx)))
            .child(self.render_open_menu(cx))
            .child(div().flex_1())
            .child(
                Button::new("open-settings")
                    .small()
                    .ghost()
                    .icon(IconName::Settings)
                    .tooltip("Settings (Ctrl+,)")
                    .on_click(|_, window, cx| settings::open(window, cx)),
            )
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
        div()
            .id(("project", index))
            .relative()
            .size(dp(38.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(dp(10.))
            .cursor_pointer()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .when(is_active, |el| el.bg(theme.primary).text_color(theme.primary_foreground))
            .when(!is_active, |el| el.bg(tint).text_color(theme.foreground).hover(|el| el.bg(mix(tint, theme.primary, 0.25))))
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
            })
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
                    .item(item("Import OpenAPI…", Workspace::prompt_import))
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
    mix(theme.title_bar, hue_color, 0.28)
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
