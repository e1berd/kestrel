mod actions;
mod view;

use std::path::PathBuf;

use gpui::{App, AppContext as _, ParentElement as _, Window};
use gpui_component::WindowExt as _;
use volley_codegen::{Generated, OutputOptions};
use volley_core::transfer::Progress;

use crate::appearance::dp;

struct CodegenDialog {
    source: PathBuf,
    destination: Option<PathBuf>,
    progress: Progress,
    busy: bool,
    error: Option<String>,
    report: Option<Generated>,
    output: OutputOptions,
}

pub fn open(source: PathBuf, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let view = cx.new(|_| CodegenDialog {
        source,
        destination: None,
        progress: Default::default(),
        busy: false,
        error: None,
        report: None,
        output: Default::default(),
    });
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Generate TypeScript SDK").w(dp(640.)).child(view.clone()));
}

impl Drop for CodegenDialog {
    fn drop(&mut self) {
        if self.busy {
            self.progress.cancel();
        }
    }
}
