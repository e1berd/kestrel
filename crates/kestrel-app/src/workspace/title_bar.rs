use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, img, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{Icon, Sizable as _, h_flex};
use gpui_kit_assets::IconName;

use super::Workspace;
use super::command_palette;
use crate::appearance::{self, dp};
use crate::assets;
use crate::settings;
use crate::ui::{ReportUnlessGone as _, keys};

impl Workspace {
    pub(super) fn render_title(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let project_name = self.active_view().map(|view| view.read(cx).name(cx));
        h_flex()
            .w_full()
            .pr_2()
            .gap_3()
            .items_center()
            .child(
                h_flex()
                    .flex_none()
                    .min_w(dp(220.))
                    .gap_2()
                    .items_center()
                    .child(img(assets::LOGO).size(dp(20.)).flex_none())
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Kestrel"))
                    .when_some(project_name, |el, name| {
                        el.child(div().text_color(palette.faint).child("/")).child(div().text_color(palette.muted).child(name))
                    }),
            )
            .child(h_flex().flex_1().justify_center().child(self.render_search_button(cx)))
            .child(self.render_env_picker(cx))
            .child(
                div()
                    .id("title-settings")
                    .size(dp(26.))
                    .rounded(dp(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(palette.muted)
                    .hover(|el| el.bg(palette.hover))
                    .child(Icon::new(IconName::Settings).small())
                    .on_click(|_, window, cx| settings::open(window, cx)),
            )
    }

    fn render_search_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let workspace = cx.entity().downgrade();
        h_flex()
            .id("command-search")
            .w_full()
            .max_w(dp(480.))
            .h(dp(26.))
            .px(dp(10.))
            .gap_2()
            .items_center()
            .rounded(dp(8.))
            .border_1()
            .border_color(palette.border)
            .bg(palette.background)
            .text_color(palette.faint)
            .cursor_pointer()
            .hover(|el| el.border_color(palette.muted))
            .child(Icon::new(IconName::Search).small())
            .child(div().flex_1().text_size(rems(0.84)).child("Find a request or a command"))
            .child(keys(&shortcut("K"), cx))
            .on_click(move |_, window, cx| {
                workspace
                    .update(cx, |workspace, cx| command_palette::open(workspace, window, cx))
                    .report_unless_gone("open the command palette");
            })
    }

    fn render_env_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.active_view() else { return div().into_any_element() };
        let palette = appearance::palette(cx);
        let project = view.read(cx).project.clone();
        let read = project.read(cx);
        let current = read.env.clone();
        let names: Vec<String> = read.collection.environments.iter().map(|env| env.name()).collect();
        let dot = if current.is_some() { palette.success } else { palette.faint };
        let label = current.clone().unwrap_or_else(|| "No environment".into());
        let view = view.downgrade();
        let content = h_flex()
            .gap(dp(7.))
            .items_center()
            .text_color(palette.text)
            .child(div().size(dp(7.)).rounded_full().bg(dot))
            .child(label)
            .child(Icon::new(gpui_component::IconName::ChevronDown).xsmall().text_color(palette.faint));
        Button::new("environment")
            .ghost()
            .h(dp(26.))
            .px(dp(10.))
            .rounded_full()
            .border_1()
            .border_color(palette.border)
            .bg(palette.background)
            .child(content)
            .dropdown_menu(move |menu, _, _| {
                let pick = |name: Option<String>, label: String, is_checked: bool| {
                    let project = project.clone();
                    PopupMenuItem::new(label).checked(is_checked).on_click(move |_, _, cx| {
                        let name = name.clone();
                        project.update(cx, |project, cx| project.set_env(name, cx));
                    })
                };
                let mut menu = menu.item(pick(None, "No environment".into(), current.is_none()));
                for name in &names {
                    menu = menu.item(pick(Some(name.clone()), name.clone(), current.as_deref() == Some(name)));
                }
                let view = view.clone();
                menu.separator().item(PopupMenuItem::new("Edit environments…").on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.open_environments(window, cx)).report_unless_gone("open the environments");
                }))
            })
            .into_any_element()
    }
}

pub fn shortcut(key: &str) -> String {
    if cfg!(target_os = "macos") { format!("⌘ {key}") } else { format!("Ctrl {key}") }
}
