use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AppContext as _, Context, ScrollStrategy, Window};

use super::ProjectView;
use crate::collection_tree::SidebarTree;
use crate::ui::ReportUnlessGone as _;

impl ProjectView {
    pub(super) fn reveal_request(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_sidebar(cx);
        self.expand_ancestors(path, cx);
        if self.filtering || self.sidebar.row_position(path).is_none() {
            self.filter_task = None;
            self.filtering = false;
            self.filter.update(cx, |filter, cx| filter.set_value("", window, cx));
            self.sidebar.show_expanded(&self.expanded);
        }
        if let Some(position) = self.sidebar.row_position(path) {
            self.tree_scroll.scroll_to_item(position, ScrollStrategy::Nearest);
        }
    }

    pub(super) fn refresh_sidebar(&mut self, cx: &mut Context<Self>) {
        let project = self.project.read(cx);
        if Arc::ptr_eq(&self.sidebar.index, &project.tree) {
            return;
        }
        self.tree_selection.retain_existing(&project.collection);
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
        self.tree_selection.select(path);
        self.sidebar.toggle(path, &mut self.expanded);
        cx.notify();
    }
}
