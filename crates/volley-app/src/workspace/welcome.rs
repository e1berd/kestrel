use std::path::{Path, PathBuf};

use gpui::{AppContext as _, Context, PathPromptOptions, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use volley_core::Collection;
use volley_openapi::GroupBy;

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
        let spec_prompt =
            PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("OpenAPI spec (JSON or YAML)".into()) };
        let picked_spec = cx.prompt_for_paths(spec_prompt);
        cx.spawn_in(window, async move |workspace, cx| {
            let Ok(Ok(Some(specs))) = picked_spec.await else { return };
            let Some(spec_path) = specs.into_iter().next() else { return };
            let Ok(picked_dir) = workspace.update(cx, |_, cx| cx.prompt_for_paths(folder_prompt("Make the collection in this folder")))
            else {
                return;
            };
            let Ok(Ok(Some(dirs))) = picked_dir.await else { return };
            let Some(dir) = dirs.into_iter().next() else { return };
            let imported = cx.background_spawn(async move { import_spec(&spec_path, &dir) }).await;
            workspace
                .update_in(cx, |workspace, window, cx| match imported {
                    Ok((root, summary)) => {
                        workspace.open_collection(&root, window, cx);
                        window.push_notification(Notification::success(format!("Imported: {summary}")), cx);
                    }
                    Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
                })
                .report_unless_gone("import an OpenAPI spec");
        })
        .detach();
    }
}

fn import_spec(spec_path: &Path, dir: &Path) -> anyhow::Result<(PathBuf, String)> {
    let spec = volley_openapi::load(&spec_path.display().to_string(), Path::new("/"))?;
    let source = relative(spec_path, dir).unwrap_or_else(|| spec_path.display().to_string());
    let (collection, report) = volley_openapi::import(dir, &source, &spec, GroupBy::Tags, None)?;
    Ok((collection.root, report.to_string()))
}

fn relative(path: &Path, base: &Path) -> Option<String> {
    let path: Vec<_> = path.components().collect();
    let base: Vec<_> = base.components().collect();
    let common = path.iter().zip(&base).take_while(|(a, b)| a == b).count();
    if common <= 1 {
        return None;
    }
    let mut out = PathBuf::new();
    for _ in common..base.len() {
        out.push("..");
    }
    for part in &path[common..] {
        out.push(part);
    }
    Some(out.display().to_string())
}
