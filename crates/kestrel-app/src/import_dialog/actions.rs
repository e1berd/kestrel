use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{AppContext as _, Context, PathPromptOptions, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use kestrel_core::report::report_error;
use kestrel_import::PreparedImport;

use super::ImportDialog;
use crate::ui::ReportUnlessGone as _;

impl ImportDialog {
    pub(super) fn choose_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose an API specification or project export".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    view.update(cx, |view, cx| view.fail(error, cx)).report_unless_gone("show file picker error");
                    None
                }
                Err(error) => {
                    report_error("choose an import file", error);
                    None
                }
            };
            if let Some(path) = path {
                view.update_in(cx, |view, window, cx| {
                    view.source = Some(path);
                    view.load_source(window, cx);
                })
                .report_unless_gone("load the import file");
            }
        })
        .detach();
    }

    pub(super) fn load_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepared = None;
        self.error = None;
        let Some(source) = self.source.clone() else {
            cx.notify();
            return;
        };
        self.busy = true;
        self.progress = Default::default();
        self.watch_progress(window, cx);
        let progress = self.progress.clone();
        let format = self.format;
        let group_by = self.group_by;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let prepared = cx
                .background_spawn(async move {
                    PreparedImport::load_with_progress(&source.to_string_lossy(), Path::new("/"), format, group_by, &progress)
                })
                .await;
            view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match prepared {
                    Ok(prepared) => {
                        view.open_after_import = prepared.preview.open_after_import_by_default();
                        view.name.update(cx, |input, cx| input.set_value(prepared.preview.title.clone(), window, cx));
                        view.prepared = Some(Arc::new(prepared));
                    }
                    Err(error) => view.fail(error, cx),
                }
                cx.notify();
            })
            .report_unless_gone("preview the import");
        })
        .detach();
    }

    pub(super) fn choose_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose an empty folder for the collection".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let destination: Option<PathBuf> = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    view.update(cx, |view, cx| view.fail(error, cx)).report_unless_gone("show folder picker error");
                    None
                }
                Err(error) => {
                    report_error("choose an import destination", error);
                    None
                }
            };
            if let Some(destination) = destination {
                view.update(cx, |view, cx| {
                    view.destination = Some(destination);
                    view.error = None;
                    cx.notify();
                })
                .report_unless_gone("choose the import destination");
            }
        })
        .detach();
    }

    pub(super) fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(prepared), Some(destination)) = (self.prepared.clone(), self.destination.clone()) else { return };
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.error = Some("Enter a collection name.".into());
            cx.notify();
            return;
        }
        self.busy = true;
        self.progress = Default::default();
        self.watch_progress(window, cx);
        let progress = self.progress.clone();
        self.error = None;
        let workspace = self.workspace.clone();
        let requests = prepared.preview.requests;
        let files = prepared.preview.requests + prepared.preview.folders + 1;
        let open_after_import = self.open_after_import;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let imported = cx
                .background_spawn(async move {
                    let root = prepared.save_with_progress(&destination, Some(&name), &progress)?;
                    if !open_after_import {
                        return Ok::<_, anyhow::Error>((root, None));
                    }
                    progress.start("Opening imported collection", Some(files as u64), kestrel_core::transfer::Unit::Files)?;
                    let collection = kestrel_core::Collection::open_with_progress(&root, &progress)?;
                    progress.finish()?;
                    Ok::<_, anyhow::Error>((root, Some(crate::project::IndexedCollection::new(collection))))
                })
                .await;
            match imported {
                Ok((root, collection)) => {
                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            window.close_dialog(cx);
                            if let Some(collection) = collection {
                                workspace.open_loaded_collection(collection, window, cx);
                            }
                            window.push_notification(
                                Notification::success(format!("Imported {requests} requests to {}", root.display())),
                                cx,
                            );
                        })
                        .report_unless_gone("open the imported collection");
                }
                Err(error) => {
                    view.update(cx, |view, cx| {
                        view.busy = false;
                        view.fail(error, cx);
                    })
                    .report_unless_gone("show the import error");
                }
            }
        })
        .detach();
    }

    fn fail(&mut self, error: impl std::fmt::Display, cx: &mut Context<Self>) {
        report_error("import a collection", &error);
        self.error = Some(format!("{error:#}"));
        cx.notify();
    }

    fn watch_progress(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(60)).await;
                match view.update(cx, |view, cx| {
                    cx.notify();
                    view.busy
                }) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        Err::<(), _>(error).report_unless_gone("update import progress");
                        break;
                    }
                }
            }
        })
        .detach();
    }
}
