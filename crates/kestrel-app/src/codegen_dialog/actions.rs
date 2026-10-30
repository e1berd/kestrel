use gpui::{AppContext as _, Context, PathPromptOptions, Window};
use kestrel_core::report::report_error;

use super::CodegenDialog;
use crate::ui::ReportUnlessGone as _;

impl CodegenDialog {
    pub(super) fn choose_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.output.single_file && !self.output.archive {
            self.choose_directory(window, cx);
            return;
        }
        let suggested = if self.output.archive { "sdk.zip" } else { "client.js" };
        let picked = cx.prompt_for_new_path(self.source.parent().unwrap_or(&self.source), Some(suggested));
        cx.spawn_in(window, async move |view, cx| match picked.await {
            Ok(Ok(Some(destination))) => view
                .update(cx, |view, cx| {
                    view.destination = Some(destination);
                    view.error = None;
                    view.report = None;
                    cx.notify();
                })
                .report_unless_gone("choose SDK generation destination"),
            Ok(Ok(None)) => {}
            Ok(Err(error)) => view.update(cx, |view, cx| view.fail(error, cx)).report_unless_gone("show SDK generation picker error"),
            Err(error) => report_error("choose SDK generation destination", error),
        })
        .detach();
    }

    fn choose_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose an empty folder or an existing generated SDK".into()),
        });
        cx.spawn_in(window, async move |view, cx| match picked.await {
            Ok(Ok(Some(paths))) => {
                if let Some(destination) = paths.into_iter().next() {
                    view.update(cx, |view, cx| {
                        view.destination = Some(destination);
                        view.error = None;
                        view.report = None;
                        cx.notify();
                    })
                    .report_unless_gone("choose SDK directory");
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => view.update(cx, |view, cx| view.fail(error, cx)).report_unless_gone("show SDK directory picker error"),
            Err(error) => report_error("choose SDK directory", error),
        })
        .detach();
    }

    pub(super) fn generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(destination) = self.destination.clone() else { return };
        self.progress = Default::default();
        self.busy = true;
        self.error = None;
        self.report = None;
        let source = self.source.clone();
        let progress = self.progress.clone();
        let options = self.output;
        self.watch_progress(window, cx);
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result =
                cx.background_spawn(async move { kestrel_codegen::generate_to(&source, &destination, options, true, &progress) }).await;
            view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(report) => view.report = Some(report),
                    Err(error) => view.fail(error, cx),
                }
                cx.notify();
            })
            .report_unless_gone("finish generating an SDK");
        })
        .detach();
    }

    fn fail(&mut self, error: impl std::fmt::Display, cx: &mut Context<Self>) {
        report_error("generate an SDK", &error);
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
                        Err::<(), _>(error).report_unless_gone("update SDK generation progress");
                        break;
                    }
                }
            }
        })
        .detach();
    }
}
