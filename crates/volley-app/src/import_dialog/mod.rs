mod actions;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, AppContext as _, Entity, ParentElement as _, WeakEntity, Window};
use gpui_component::WindowExt as _;
use gpui_component::input::InputState;
use volley_import::{Format, PreparedImport};
use volley_openapi::GroupBy;

use crate::appearance::dp;
use crate::workspace::Workspace;

struct ImportDialog {
    workspace: WeakEntity<Workspace>,
    format: Option<Format>,
    group_by: GroupBy,
    source: Option<PathBuf>,
    destination: Option<PathBuf>,
    prepared: Option<Arc<PreparedImport>>,
    name: Entity<InputState>,
    busy: bool,
    error: Option<String>,
    progress: volley_core::transfer::Progress,
    open_after_import: bool,
}

pub fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let name = cx.new(|cx| InputState::new(window, cx).placeholder("Collection name"));
    let view = cx.new(|_| ImportDialog {
        workspace,
        format: None,
        group_by: GroupBy::Tags,
        source: None,
        destination: None,
        prepared: None,
        name,
        busy: false,
        error: None,
        progress: Default::default(),
        open_after_import: true,
    });
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Import collection").w(dp(640.)).child(view.clone()));
}

impl Drop for ImportDialog {
    fn drop(&mut self) {
        if self.busy {
            self.progress.cancel();
        }
    }
}
