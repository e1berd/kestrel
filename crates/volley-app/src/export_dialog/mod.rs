mod actions;
mod view;

use std::path::PathBuf;

use gpui::{App, AppContext as _, ParentElement as _, Window};
use gpui_component::WindowExt as _;
use volley_core::transfer::Progress;
use volley_import::export::{Encoding, ExportFormat, ExportReport};

use crate::appearance::dp;

struct ExportDialog {
    source: PathBuf,
    destination: Option<PathBuf>,
    format: ExportFormat,
    encoding: Encoding,
    progress: Progress,
    busy: bool,
    error: Option<String>,
    report: Option<ExportReport>,
}

pub fn open(source: PathBuf, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let view = cx.new(|_| ExportDialog {
        source,
        destination: None,
        format: ExportFormat::OpenApi32,
        encoding: Encoding::Json,
        progress: Default::default(),
        busy: false,
        error: None,
        report: None,
    });
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Export collection").w(dp(640.)).child(view.clone()));
}

impl Drop for ExportDialog {
    fn drop(&mut self) {
        if self.busy {
            self.progress.cancel();
        }
    }
}
