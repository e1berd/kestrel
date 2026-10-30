use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;
use kestrel_core::Item;

use super::ProjectView;
use super::dragging::TreeDrag;
use crate::appearance::{self, dp};
use crate::ui::{ReportUnlessGone as _, method_badge};

struct Row {
    path: PathBuf,
    name: String,
    method: Option<String>,
    depth: usize,
    is_open: bool,
    count: usize,
}

type TreeAction = fn(&mut ProjectView, PathBuf, &mut Window, &mut Context<ProjectView>);

impl ProjectView {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let project = self.project.read(cx);
        let filter = self.filter.read(cx).value().to_lowercase();
        let mut rows = Vec::new();
        self.collect_rows(&project.collection.items, 0, &filter, &mut rows);
        let active_path = self.active_tab().map(|tab| tab.read(cx).path.clone());
        let load_error = project.load_error.clone();
        v_flex()
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
            .child(
                div().id("tree").flex_1().min_h_0().overflow_y_scroll().pb_4().child(
                    v_flex()
                        .min_h_full()
                        .gap(dp(1.))
                        .children(rows.into_iter().map(|row| self.render_row(row, active_path.as_deref(), cx)))
                        .child(self.render_root_drop_zone(cx)),
                ),
            )
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

    fn collect_rows(&self, items: &[Item], depth: usize, filter: &str, out: &mut Vec<Row>) {
        for item in items {
            match item {
                Item::Folder(folder) => {
                    let is_open = !filter.is_empty() || self.expanded.contains(&folder.dir);
                    let mut inner = Vec::new();
                    if is_open {
                        self.collect_rows(&folder.children, depth + 1, filter, &mut inner);
                    }
                    if !filter.is_empty() && inner.is_empty() && !folder.name().to_lowercase().contains(filter) {
                        continue;
                    }
                    let count = count_requests(&folder.children);
                    out.push(Row { path: folder.dir.clone(), name: folder.name(), method: None, depth, is_open, count });
                    out.extend(inner);
                }
                Item::Request(request) => {
                    let name = request.name();
                    let url = request.doc.data.http.as_ref().map(|http| http.url.to_lowercase()).unwrap_or_default();
                    if !filter.is_empty() && !name.to_lowercase().contains(filter) && !url.contains(filter) {
                        continue;
                    }
                    out.push(Row { path: request.doc.path.clone(), name, method: Some(request.method()), depth, is_open: false, count: 0 });
                }
            }
        }
    }

    fn render_row(&self, row: Row, active: Option<&Path>, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let active_background = cx.theme().list_active;
        let is_active = active == Some(row.path.as_path());
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
            Some(method) => method_badge(method, cx).into_any_element(),
        };
        let view = cx.entity().downgrade();
        let menu_path = path.clone();
        let drag = TreeDrag { root: self.root(cx), path: path.clone(), name: row.name.clone(), method: row.method.clone() };
        h_flex()
            .id(ElementId::Name(SharedString::from(format!("row-{}", row.path.display()))))
            .relative()
            .w_full()
            .h(rems(2.0))
            .pl(dp(indent))
            .pr(dp(8.))
            .gap(dp(8.))
            .items_center()
            .rounded(dp(8.))
            .cursor_pointer()
            .text_size(rems(0.86))
            .text_color(if is_folder || is_active { palette.text } else { palette.muted })
            .when(is_active, |el| el.bg(active_background).font_weight(gpui::FontWeight::MEDIUM))
            .when(!is_active, |el| el.hover(|el| el.bg(palette.hover).text_color(palette.text)))
            .child(lead)
            .child(div().flex_1().min_w_0().truncate().when(is_folder, |el| el.font_weight(gpui::FontWeight::MEDIUM)).child(row.name))
            .when(is_folder && row.count > 0, |el| {
                el.child(div().text_size(rems(0.72)).text_color(palette.faint).child(row.count.to_string()))
            })
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .when(cx.has_active_drag(), |element| element.child(self.render_drop_zones(&row.path, is_folder, cx)))
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                if is_folder {
                    view.toggle_folder(&path, cx);
                } else {
                    view.open_request(&path, window, cx);
                }
            }))
            .context_menu(move |menu, _, _| row_menu(menu, view.clone(), menu_path.clone(), is_folder))
            .into_any_element()
    }

    fn toggle_folder(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
        }
        cx.notify();
    }
}

fn count_requests(items: &[Item]) -> usize {
    items
        .iter()
        .map(|item| match item {
            Item::Folder(folder) => count_requests(&folder.children),
            Item::Request(_) => 1,
        })
        .sum()
}

fn row_menu(menu: PopupMenu, view: gpui::WeakEntity<ProjectView>, path: PathBuf, is_folder: bool) -> PopupMenu {
    let item = |label: &'static str, action: TreeAction| {
        let view = view.clone();
        let path = path.clone();
        PopupMenuItem::new(label).on_click(move |_, window, cx| {
            let path = path.clone();
            view.update(cx, |view, cx| action(view, path, window, cx)).report_unless_gone("run a collection tree action");
        })
    };
    let menu = if is_folder {
        menu.item(item("New request", |view, path, window, cx| view.new_request(Some(path), window, cx)))
            .item(item("New folder", |view, path, window, cx| view.new_folder(Some(path), window, cx)))
            .separator()
    } else {
        menu.item(item("Open", |view, path, window, cx| view.open_request(&path, window, cx)))
            .item(item("Duplicate", |view, path, window, cx| view.duplicate(path, window, cx)))
    };
    menu.item(item("Rename…", |view, path, window, cx| view.rename(path, window, cx)))
        .item(item("Show in file manager", |_, path, _, cx| cx.reveal_path(&path)))
        .separator()
        .item(item("Delete…", |view, path, window, cx| view.delete(path, window, cx)))
}

fn collection_menu(menu: PopupMenu, view: gpui::WeakEntity<ProjectView>) -> PopupMenu {
    let item = |label: &'static str, action: fn(&mut ProjectView, &mut Window, &mut Context<ProjectView>)| {
        let view = view.clone();
        PopupMenuItem::new(label).on_click(move |_, window, cx| {
            view.update(cx, |view, cx| action(view, window, cx)).report_unless_gone("run a collection action");
        })
    };
    menu.item(item("New folder", |view, window, cx| view.new_folder(None, window, cx)))
        .item(item("Environments…", |view, window, cx| view.open_environments(window, cx)))
        .item(item("Sync from OpenAPI", |view, window, cx| view.sync_openapi(window, cx)))
        .item(item("Export collection…", |view, window, cx| crate::export_dialog::open(view.root(cx), window, cx)))
        .item(item("Generate SDK…", |view, window, cx| crate::codegen_dialog::open(view.root(cx), window, cx)))
        .separator()
        .item(item("Reload from disk", |view, _, cx| view.project.update(cx, |project, cx| project.reload(cx))))
        .item(item("Show in file manager", |view, _, cx| reveal_collection(view, cx)))
}

fn reveal_collection(view: &ProjectView, cx: &mut App) {
    cx.reveal_path(&view.root(cx).join(kestrel_core::collection::COLLECTION_FILE));
}
