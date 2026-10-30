use std::path::{Path, PathBuf};

use gpui::{AppContext as _, Context, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use kestrel_core::report::report_error;

use super::{ProjectViewEvent, Workspace};
use crate::prefs::Prefs;
use crate::project::{IndexedCollection, Project};
use crate::project_view::ProjectView;
use crate::ui::ReportUnlessGone as _;

impl Workspace {
    pub(super) fn restore(&mut self, initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let prefs = Prefs::load();
        let focus = initial.clone().or(prefs.active_project);
        let mut paths = prefs.open_projects;
        if let Some(path) = &focus {
            paths.retain(|saved| saved != path);
            paths.push(path.clone());
        }
        self.opening.extend(paths.clone());
        let task = cx.background_spawn(async move {
            paths
                .into_iter()
                .map(|path| {
                    let explicit = initial.as_ref() == Some(&path);
                    let result = if explicit || path.join(kestrel_core::collection::COLLECTION_FILE).is_file() {
                        Some(IndexedCollection::open(&path))
                    } else {
                        None
                    };
                    (path, result)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |workspace, cx| {
            let loaded = task.await;
            workspace
                .update_in(cx, |workspace, window, cx| {
                    for (path, result) in loaded {
                        workspace.opening.remove(&path);
                        if let Some(result) = result {
                            workspace.finish_open(&path, result, window, cx);
                        }
                    }
                    cx.notify();
                })
                .report_unless_gone("restore open collections");
        })
        .detach();
    }

    pub fn open_collection(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.views.iter().position(|view| view.read(cx).root(cx) == path) {
            self.activate(index, window, cx);
            return;
        }
        if !self.opening.insert(path.to_path_buf()) {
            return;
        }
        let path = path.to_path_buf();
        let source = path.clone();
        let task = cx.background_spawn(async move { IndexedCollection::open(&source) });
        cx.notify();
        cx.spawn_in(window, async move |workspace, cx| {
            let result = task.await;
            workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.opening.remove(&path);
                    workspace.finish_open(&path, result, window, cx);
                    cx.notify();
                })
                .report_unless_gone("open a collection");
        })
        .detach();
    }

    fn finish_open(&mut self, path: &Path, result: anyhow::Result<IndexedCollection>, window: &mut Window, cx: &mut Context<Self>) {
        match result {
            Ok(collection) => {
                self.open_loaded_collection(collection, window, cx);
                if path.extension().is_some_and(|extension| extension == "yml" || extension == "yaml") {
                    let request = match path.canonicalize() {
                        Ok(request) => request,
                        Err(error) => return report_error("resolve the initial request path", error),
                    };
                    self.with_view(cx, |view, cx| view.open_request(&request, window, cx));
                }
            }
            Err(error) => {
                report_error("open a collection", &error);
                window.push_notification(Notification::error(format!("{error:#}")), cx);
            }
        }
    }

    pub fn open_loaded_collection(&mut self, collection: IndexedCollection, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.views.iter().position(|view| view.read(cx).root(cx) == collection.collection.root) {
            self.activate(index, window, cx);
            return;
        }
        let project = Project::from_collection(collection, self.interact.clone(), cx);
        let view = cx.new(|cx| ProjectView::new(project, window, cx));
        self._subscriptions.push(cx.subscribe(&view, |_, _, _: &ProjectViewEvent, cx| cx.notify()));
        self.views.push(view);
        self.activate(self.views.len() - 1, window, cx);
    }
}
