use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, PathPromptOptions,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::notification::Notification;
use gpui_component::{ActiveTheme as _, IconName, WindowExt as _, h_flex, v_flex};
use volley_core::Collection;
use volley_openapi::GroupBy;

use super::Workspace;
use crate::appearance::dp;
use crate::prefs::Prefs;
use crate::ui::{ReportUnlessGone as _, hint};

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

    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let recent: Vec<PathBuf> =
            Prefs::load().recent.into_iter().filter(|path| path.join(volley_core::collection::COLLECTION_FILE).is_file()).collect();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(div().text_3xl().font_weight(FontWeight::BOLD).text_color(cx.theme().primary).child("Kestrel"))
            .child(hint("A free API client that keeps your requests as files in git.", cx).text_sm())
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("welcome-open")
                            .primary()
                            .icon(IconName::FolderOpen)
                            .label("Open collection")
                            .on_click(cx.listener(|workspace, _, window, cx| workspace.prompt_open(window, cx))),
                    )
                    .child(
                        Button::new("welcome-new")
                            .outline()
                            .icon(IconName::Plus)
                            .label("New collection")
                            .on_click(cx.listener(|workspace, _, window, cx| workspace.prompt_new_collection(window, cx))),
                    )
                    .child(
                        Button::new("welcome-import")
                            .outline()
                            .icon(IconName::FileText)
                            .label("Import OpenAPI")
                            .on_click(cx.listener(|workspace, _, window, cx| workspace.prompt_import(window, cx))),
                    ),
            )
            .when(!recent.is_empty(), |el| el.child(self.render_recent(recent, cx)))
            .into_any_element()
    }

    fn render_recent(&self, recent: Vec<PathBuf>, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(dp(560.)).gap_1().pt_4().child(hint("Recent", cx)).children(recent.into_iter().enumerate().map(|(index, path)| {
            h_flex()
                .id(("recent", index))
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .hover(|el| el.bg(cx.theme().list_hover))
                .child(path.display().to_string())
                .on_click(cx.listener(move |workspace, _, window, cx| workspace.open_collection(&path, window, cx)))
        }))
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
