use gpui::{AppContext as _, Context, Window};
use volley_core::report::report_error;

use super::ExportDialog;
use crate::ui::ReportUnlessGone as _;

impl ExportDialog {
    pub(super) fn choose_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let extension = if self.encoding == volley_import::export::Encoding::Yaml { "yaml" } else { "json" };
        let suggested = format!("{}.{extension}", self.format.id());
        let picked = cx.prompt_for_new_path(self.source.parent().unwrap_or(&self.source), Some(&suggested));
        cx.spawn_in(window, async move |view, cx| match picked.await {
            Ok(Ok(Some(destination))) => view
                .update(cx, |view, cx| {
                    view.destination = Some(destination);
                    view.error = None;
                    view.report = None;
                    cx.notify();
                })
                .report_unless_gone("choose export destination"),
            Ok(Ok(None)) => {}
            Ok(Err(error)) => view.update(cx, |view, cx| view.fail(error, cx)).report_unless_gone("show export picker error"),
            Err(error) => report_error("choose export destination", error),
        })
        .detach();
    }

    pub(super) fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(destination) = self.destination.clone() else { return };
        self.progress = Default::default();
        self.busy = true;
        self.error = None;
        self.report = None;
        let source = self.source.clone();
        let format = self.format;
        let encoding = self.encoding;
        let progress = self.progress.clone();
        self.watch_progress(window, cx);
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_spawn(async move { volley_import::export::export(&source, &destination, format, encoding, true, &progress) })
                .await;
            view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(report) => view.report = Some(report),
                    Err(error) => view.fail(error, cx),
                }
                cx.notify();
            })
            .report_unless_gone("finish exporting a collection");
        })
        .detach();
    }

    fn fail(&mut self, error: impl std::fmt::Display, cx: &mut Context<Self>) {
        report_error("export a collection", &error);
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
                        Err::<(), _>(error).report_unless_gone("update export progress");
                        break;
                    }
                }
            }
        })
        .detach();
    }
}
