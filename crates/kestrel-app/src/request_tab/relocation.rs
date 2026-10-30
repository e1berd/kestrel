use std::path::Path;

use gpui::{Context, Window};

use super::RequestTab;

impl RequestTab {
    pub fn relocate(&mut self, old: &Path, new: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(relative) = self.path.strip_prefix(old) {
            self.path = new.join(relative);
            self.doc.path = self.path.clone();
        }
        let fresh = self.project.read(cx).collection.request(&self.path).map(|request| request.doc.clone());
        if let Some(fresh) = fresh {
            self.doc.data.info.seq = fresh.data.info.seq;
            if self.doc.data == fresh.data {
                self.doc = fresh;
            } else {
                self.reload_from_disk(window, cx);
            }
        }
        self.update_preview(cx);
        cx.notify();
    }
}
