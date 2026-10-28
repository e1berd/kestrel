use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use volley_core::Item;

use super::ProjectView;
use crate::appearance::dp;
use crate::ui::{ReportUnlessGone as _, method_label};

struct Row {
    path: PathBuf,
    name: String,
    method: Option<String>,
    depth: usize,
    is_open: bool,
}

type TreeAction = fn(&mut ProjectView, PathBuf, &mut Window, &mut Context<ProjectView>);

impl ProjectView {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let project = self.project.read(cx);
        let filter = self.filter.read(cx).value().to_lowercase();
        let mut rows = Vec::new();
        self.collect_rows(&project.collection.items, 0, &filter, &mut rows);
        let active_path = self.active_tab().map(|tab| tab.read(cx).path.clone());
        let load_error = project.load_error.clone();
        let theme = cx.theme();
        v_flex()
            .size_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .child(self.render_sidebar_header(cx))
            .when_some(load_error, |el, error| el.child(div().mx_2().mb_1().text_xs().text_color(cx.theme().danger).child(error)))
            .child(div().px_2().pb_2().child(Input::new(&self.filter).small().cleanable(true)))
            .child(
                div()
                    .id("tree")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_1()
                    .pb_4()
                    .children(rows.into_iter().map(|row| self.render_row(row, active_path.as_deref(), cx))),
            )
            .into_any_element()
    }

    fn render_sidebar_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        h_flex()
            .px_2()
            .py_2()
            .gap_1()
            .items_center()
            .child(div().flex_1().min_w_0().truncate().text_sm().font_weight(FontWeight::SEMIBOLD).child(self.name(cx)))
            .child(
                Button::new("add-request")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip("New request")
                    .on_click(cx.listener(|view, _, window, cx| view.new_request(None, window, cx))),
            )
            .child(
                Button::new("sync-openapi")
                    .xsmall()
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .tooltip("Sync from OpenAPI")
                    .on_click(cx.listener(|view, _, window, cx| view.sync_openapi(window, cx))),
            )
            .child(
                Button::new("collection-menu")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Ellipsis)
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
                    out.push(Row { path: folder.dir.clone(), name: folder.name(), method: None, depth, is_open });
                    out.extend(inner);
                }
                Item::Request(request) => {
                    let name = request.name();
                    let url = request.doc.data.http.as_ref().map(|http| http.url.to_lowercase()).unwrap_or_default();
                    if !filter.is_empty() && !name.to_lowercase().contains(filter) && !url.contains(filter) {
                        continue;
                    }
                    out.push(Row { path: request.doc.path.clone(), name, method: Some(request.method()), depth, is_open: false });
                }
            }
        }
    }

    fn render_row(&self, row: Row, active: Option<&Path>, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let is_active = active == Some(row.path.as_path());
        let is_folder = row.method.is_none();
        let path = row.path.clone();
        let marker: AnyElement = match &row.method {
            None => h_flex()
                .gap_1()
                .text_color(theme.muted_foreground)
                .child(Icon::new(if row.is_open { IconName::ChevronDown } else { IconName::ChevronRight }).xsmall())
                .child(Icon::new(if row.is_open { IconName::FolderOpen } else { IconName::FolderClosed }).xsmall())
                .into_any_element(),
            Some(method) => method_label(method, cx).into_any_element(),
        };
        let view = cx.entity().downgrade();
        let menu_path = path.clone();
        h_flex()
            .id(ElementId::Name(SharedString::from(format!("row-{}", row.path.display()))))
            .w_full()
            .h(rems(1.85))
            .pl(dp(8. + row.depth as f32 * 14.))
            .pr_2()
            .gap_1p5()
            .items_center()
            .text_sm()
            .rounded_md()
            .cursor_pointer()
            .when(is_active, |el| el.bg(theme.list_active).text_color(theme.foreground))
            .when(!is_active, |el| el.hover(|el| el.bg(theme.list_hover)))
            .child(marker)
            .child(div().flex_1().min_w_0().truncate().child(row.name))
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
        .separator()
        .item(item("Reload from disk", |view, _, cx| view.project.update(cx, |project, cx| project.reload(cx))))
        .item(item("Show in file manager", |view, _, cx| reveal_collection(view, cx)))
}

fn reveal_collection(view: &ProjectView, cx: &mut App) {
    cx.reveal_path(&view.root(cx).join(volley_core::collection::COLLECTION_FILE));
}
