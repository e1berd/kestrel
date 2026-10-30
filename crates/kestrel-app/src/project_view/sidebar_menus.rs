use std::path::{Path, PathBuf};

use gpui::{App, Context, WeakEntity, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use super::ProjectView;
use crate::ui::ReportUnlessGone as _;

type TreeAction = fn(&mut ProjectView, PathBuf, &mut Window, &mut Context<ProjectView>);
type CreationAction = fn(&mut ProjectView, Option<PathBuf>, &mut Window, &mut Context<ProjectView>);

pub(super) fn creation_menu(menu: PopupMenu, view: WeakEntity<ProjectView>, directory: Option<PathBuf>) -> PopupMenu {
    let item = |label: &'static str, action: CreationAction| {
        let view = view.clone();
        let directory = directory.clone();
        PopupMenuItem::new(label).on_click(move |_, window, cx| {
            view.update(cx, |view, cx| action(view, directory.clone(), window, cx)).report_unless_gone("create a collection item");
        })
    };
    menu.item(item("New request", ProjectView::new_request)).item(item("New folder", ProjectView::new_folder))
}

pub(super) fn row_menu(menu: PopupMenu, view: WeakEntity<ProjectView>, path: PathBuf, is_folder: bool) -> PopupMenu {
    let item = |label: &'static str, action: TreeAction| {
        let view = view.clone();
        let path = path.clone();
        PopupMenuItem::new(label).on_click(move |_, window, cx| {
            let path = path.clone();
            view.update(cx, |view, cx| action(view, path, window, cx)).report_unless_gone("run a collection tree action");
        })
    };
    let directory = if is_folder { Some(path.clone()) } else { path.parent().map(Path::to_path_buf) };
    let menu = creation_menu(menu, view.clone(), directory).separator();
    let menu = if is_folder {
        menu
    } else {
        menu.item(item("Open", |view, path, window, cx| view.open_request(&path, window, cx)))
            .item(item("Duplicate", |view, path, window, cx| view.duplicate(path, window, cx)))
    };
    menu.item(item("Rename…", |view, path, window, cx| view.rename(path, window, cx)))
        .item(item("Show in file manager", |_, path, _, cx| cx.reveal_path(&path)))
        .separator()
        .item(item("Delete…", |view, path, window, cx| view.delete(path, window, cx)))
}

pub(super) fn collection_menu(menu: PopupMenu, view: WeakEntity<ProjectView>) -> PopupMenu {
    let item = |label: &'static str, action: fn(&mut ProjectView, &mut Window, &mut Context<ProjectView>)| {
        let view = view.clone();
        PopupMenuItem::new(label).on_click(move |_, window, cx| {
            view.update(cx, |view, cx| action(view, window, cx)).report_unless_gone("run a collection action");
        })
    };
    creation_menu(menu, view.clone(), None)
        .separator()
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
