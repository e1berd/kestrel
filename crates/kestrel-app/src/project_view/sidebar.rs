use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, rems, uniform_list,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;

use super::ProjectView;
use super::dragging::TreeDrag;
use super::sidebar_menus::{collection_menu, creation_menu, row_menu};
use crate::appearance::{self, dp};
use crate::ui::{ReportUnlessGone as _, method_label};

#[derive(Clone)]
struct Row {
    path: PathBuf,
    name: String,
    method: Option<String>,
    depth: usize,
    is_open: bool,
    count: usize,
}

impl ProjectView {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let project = self.project.read(cx);

        let active_path = self.active_tab().map(|tab| tab.read(cx).path.clone());
        let load_error = project.load_error.clone();
        let view = cx.entity().downgrade();
        v_flex()
            .id("collection-sidebar")
            .size_full()
            .pt(dp(8.))
            .px(dp(6.))
            .gap(dp(10.))
            .child(self.render_sidebar_header(cx))
            .when_some(load_error, |el, error| el.child(div().mx_2().text_xs().text_color(palette.danger).child(error)))
            .child(
                div()
                    .px(dp(2.))
                    .child(Input::new(&self.filter).prefix(Icon::new(IconName::Search).small().text_color(palette.faint)).cleanable(true)),
            )
            .when(self.filtering, |element| element.child(crate::ui::hint("Filtering requests…", cx)))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        uniform_list(
                            "tree",
                            self.sidebar.rows.len(),
                            cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .filter_map(|index| {
                                        let node = view.sidebar.node(index)?;
                                        let row = Row {
                                            path: node.path.to_path_buf(),
                                            name: node.name.clone(),
                                            method: node.method.clone(),
                                            depth: node.depth,
                                            is_open: view.sidebar.filtered || view.expanded.contains(node.path.as_ref()),
                                            count: node.count,
                                        };
                                        Some(view.render_row(row, active_path.as_deref(), cx))
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.tree_scroll)
                        .flex_1()
                        .min_h_0(),
                    )
                    .child(self.render_root_drop_zone(cx)),
            )
            .context_menu(move |menu, _, _| creation_menu(menu, view.clone(), None))
            .into_any_element()
    }

    fn render_sidebar_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let view = cx.entity().downgrade();
        let icon_button = |id: &'static str, icon: IconName| {
            div()
                .id(id)
                .size(dp(26.))
                .rounded(dp(7.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(palette.muted)
                .hover(|el| el.bg(palette.hover).text_color(palette.text))
                .child(Icon::new(icon).small())
        };
        h_flex()
            .px(dp(6.))
            .gap(dp(2.))
            .items_center()
            .child(self.render_collection_drop_label(cx))
            .child(div().flex_1())
            .child(
                icon_button("add-request", IconName::Plus).on_click(cx.listener(|view, _, window, cx| view.new_request(None, window, cx))),
            )
            .child(
                icon_button("sync-openapi", IconName::RefreshCw).on_click(cx.listener(|view, _, window, cx| view.sync_openapi(window, cx))),
            )
            .child(
                Button::new("collection-menu")
                    .xsmall()
                    .ghost()
                    .icon(Icon::new(IconName::Ellipsis))
                    .dropdown_menu(move |menu, _, _| collection_menu(menu, view.clone())),
            )
    }

    fn render_row(&self, row: Row, active: Option<&Path>, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let active_background = cx.theme().list_active;
        let is_active = self.tree_selection.path().or(active) == Some(row.path.as_path());
        let is_folder = row.method.is_none();
        let path = row.path.clone();
        let indent = row.depth as f32 * 14. + if is_folder { 8. } else { 30. };
        let lead: AnyElement = match &row.method {
            None => h_flex()
                .gap(dp(6.))
                .text_color(palette.faint)
                .child(
                    Icon::new(gpui_component::IconName::ChevronRight)
                        .size(dp(12.))
                        .when(row.is_open, |icon| icon.rotate(gpui::Radians(std::f32::consts::FRAC_PI_2))),
                )
                .child(Icon::new(if row.is_open { IconName::FolderOpen } else { IconName::Folder }).size(dp(14.)))
                .into_any_element(),
            Some(method) => method_label(method, cx).into_any_element(),
        };
        let view = cx.entity().downgrade();
        let drag_view = view.clone();
        let menu_path = path.clone();
        let drag = TreeDrag { root: self.root(cx), path: path.clone(), name: row.name.clone(), method: row.method.clone() };
        h_flex()
            .id(ElementId::Name(SharedString::from(format!("row-{}", row.path.display()))))
            .relative()
            .w_full()
            .h(rems(2.0))
            .pl(dp(indent))
            .pr(dp(8.))
            .gap(dp(if is_folder { 8. } else { 4. }))
            .items_center()
            .rounded(dp(8.))
            .cursor_pointer()
            .text_size(rems(0.86))
            .text_color(if is_folder || is_active { palette.text } else { palette.muted })
            .when(is_active, |el| el.bg(active_background).font_weight(gpui::FontWeight::MEDIUM))
            .when(!is_active, |el| el.hover(|el| el.bg(palette.hover).text_color(palette.text)))
            .child(lead)
            .child(
                div().flex_1().min_w_0().truncate().when(is_folder, |el| el.font_weight(gpui::FontWeight::MEDIUM)).child(row.name.clone()),
            )
            .when(is_folder && row.count > 0, |el| {
                el.child(div().text_size(rems(0.72)).text_color(palette.faint).child(row.count.to_string()))
            })
            .on_drag(drag, move |drag, _, _, cx| {
                drag_view.update(cx, |view, _| view.tree_hover = None).report_unless_gone("start dragging a collection item");
                cx.new(|_| drag.clone())
            })
            .when(cx.has_active_drag(), |element| element.child(self.render_drop_zones(&row.path, &row.name, is_folder, cx)))
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                if is_folder {
                    view.toggle_folder(&path, cx);
                } else {
                    view.open_request(&path, window, cx);
                }
            }))
            .context_menu(move |menu, _, cx| {
                view.update(cx, |view, cx| {
                    view.tree_selection.select(&menu_path);
                    cx.notify();
                })
                .report_unless_gone("select a collection item");
                row_menu(menu, view.clone(), menu_path.clone(), is_folder)
            })
            .into_any_element()
    }
}
