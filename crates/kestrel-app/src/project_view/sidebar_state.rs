use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AppContext as _, Context};

use super::ProjectView;
use crate::collection_tree::SidebarTree;
use crate::ui::ReportUnlessGone as _;

impl ProjectView {
    pub(super) fn refresh_sidebar(&mut self, cx: &mut Context<Self>) {
        let project = self.project.read(cx);
        if Arc::ptr_eq(&self.sidebar.index, &project.tree) {
            return;
        }
        self.sidebar = SidebarTree::new(project.tree.clone(), &self.expanded);
        self.schedule_filter(cx);
    }

    pub(super) fn schedule_filter(&mut self, cx: &mut Context<Self>) {
        self.filter_task = None;
        let query = self.filter.read(cx).value().to_lowercase();
        self.filtering = !query.is_empty();
        if query.is_empty() {
            self.sidebar.show_expanded(&self.expanded);
        } else {
            let index = self.sidebar.index.clone();
            self.filter_task = Some(cx.spawn(async move |view, cx| {
                cx.background_executor().timer(Duration::from_millis(120)).await;
                let searchable = index.clone();
                let search = query.clone();
                let rows = cx.background_spawn(async move { searchable.matching_rows(&search) }).await;
                view.update(cx, |view, cx| {
                    if Arc::ptr_eq(&index, &view.sidebar.index) && view.filter.read(cx).value().to_lowercase() == query {
                        view.sidebar.show_matches(rows);
                        view.filtering = false;
                        cx.notify();
                    }
                })
                .report_unless_gone("filter the collection tree");
            }));
        }
        cx.notify();
    }

    pub(super) fn toggle_folder(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.sidebar.toggle(path, &mut self.expanded);
        cx.notify();
    }
}
