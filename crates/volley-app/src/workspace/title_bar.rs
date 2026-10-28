use gpui::{AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div};
use gpui_component::button::Button;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, h_flex};

use super::Workspace;
use crate::appearance::dp;
use crate::ui::ReportUnlessGone as _;

impl Workspace {
    pub(super) fn render_title(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let project_name = self.active_view().map(|view| view.read(cx).name(cx));
        h_flex()
            .w_full()
            .pr_2()
            .gap_2()
            .items_center()
            .child(div().text_sm().font_weight(FontWeight::BOLD).text_color(cx.theme().primary).child("Kestrel"))
            .children(project_name.map(|name| div().text_sm().text_color(cx.theme().muted_foreground).child(name)))
            .child(div().flex_1())
            .child(self.render_env_picker(cx))
            .child(div().w(dp(8.)))
    }

    fn render_env_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.active_view() else { return div().into_any_element() };
        let project = view.read(cx).project.clone();
        let read = project.read(cx);
        let current = read.env.clone();
        let names: Vec<String> = read.collection.environments.iter().map(|env| env.name()).collect();
        let label = current.clone().unwrap_or_else(|| "No environment".into());
        let view = view.downgrade();
        Button::new("environment")
            .small()
            .outline()
            .icon(IconName::Globe)
            .label(label)
            .dropdown_caret(true)
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
