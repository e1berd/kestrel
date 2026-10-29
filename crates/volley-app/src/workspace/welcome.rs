use gpui::{Context, PathPromptOptions, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use volley_core::Collection;

use super::Workspace;
use crate::ui::ReportUnlessGone as _;

fn folder_prompt(prompt: &str) -> PathPromptOptions {
    PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(prompt.to_string().into()) }
}

impl Workspace {
    pub(super) fn prompt_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(folder_prompt("Open collection"));
        cx.spawn_in(window, async move |workspace, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            workspace
                .update_in(cx, |workspace, window, cx| workspace.open_collection(&path, window, cx))
                .report_unless_gone("open a collection");
        })
        .detach();
    }

    pub(super) fn prompt_new_collection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(folder_prompt("Make the collection in this folder"));
        cx.spawn_in(window, async move |workspace, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(dir) = paths.into_iter().next() else { return };
            workspace
                .update_in(cx, |workspace, window, cx| match Collection::create(&dir, &volley_core::collection::file_stem(&dir)) {
                    Ok(_) => workspace.open_collection(&dir, window, cx),
                    Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
                })
                .report_unless_gone("make a collection");
        })
        .detach();
    }

    pub(super) fn prompt_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::import_dialog::open(cx.entity().downgrade(), window, cx);
    }

    pub(super) fn prompt_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.active_view() {
            crate::export_dialog::open(view.read(cx).root(cx), window, cx);
        } else {
            window.push_notification(Notification::error("Open a collection to export it"), cx);
        }
    }
}
