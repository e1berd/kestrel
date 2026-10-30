use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gpui::{AppContext as _, Context, ParentElement as _, Window};
use gpui_component::WindowExt as _;
use kestrel_core::Item;
use kestrel_core::format::RequestFile;

use super::ProjectView;
use crate::appearance::dp;
use crate::dialogs::{ask_text, confirm};
use crate::env_editor::EnvEditor;
use crate::ui::ReportUnlessGone as _;

impl ProjectView {
    pub fn new_request(&mut self, dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let dir = dir.unwrap_or_else(|| self.tree_selection.directory(&self.project.read(cx).collection));
        let view = cx.entity().downgrade();
        ask_text(window, cx, "New request", "Request name", "", move |name, window, cx| {
            let dir = dir.clone();
            view.update(cx, |view, cx| {
                let created = view.project.update(cx, |project, _| {
                    project.edit_tree(|collection| collection.add_request(&dir, RequestFile::new_http(&name, "GET", "{{baseUrl}}/")))
                });
                match created {
                    Ok(path) => {
                        view.expanded.insert(dir.clone());
                        view.open_request(&path, window, cx);
                    }
                    Err(error) => Self::notify_error(window, cx, error),
                }
            })
            .report_unless_gone("make a request");
        });
    }

    pub fn new_folder(&mut self, parent: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let parent = parent.unwrap_or_else(|| self.tree_selection.directory(&self.project.read(cx).collection));
        let view = cx.entity().downgrade();
        ask_text(window, cx, "New folder", "Folder name", "", move |name, window, cx| {
            let parent = parent.clone();
            view.update(cx, |view, cx| {
                match view.project.update(cx, |project, _| project.edit_tree(|collection| collection.add_folder(&parent, &name))) {
                    Ok(dir) => {
                        view.expanded.insert(parent.clone());
                        view.tree_selection.select(&dir);
                        view.expanded.insert(dir);
                        cx.notify();
                    }
                    Err(error) => Self::notify_error(window, cx, error),
                }
            })
            .report_unless_gone("make a folder");
        });
    }

    pub fn rename(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.project.read(cx).collection.item(&path).map(Item::name) else { return };
        let view = cx.entity().downgrade();
        ask_text(window, cx, "Rename", "Name", current, move |name, window, cx| {
            let path = path.clone();
            view.update(cx, |view, cx| {
                match view.project.update(cx, |project, _| project.edit_tree(|collection| collection.rename(&path, &name))) {
                    Ok(new_path) => view.reopen_moved_tabs(&path, &new_path, window, cx),
                    Err(error) => Self::notify_error(window, cx, error),
                }
            })
            .report_unless_gone("rename an item");
        });
    }

    fn reopen_moved_tabs(&mut self, old: &Path, new: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.tree_selection.relocate(old, new);
        let selected_path = self.tree_selection.path().map(Path::to_path_buf);
        let moved: Vec<PathBuf> = self
            .tabs
            .iter()
            .map(|tab| tab.read(cx).path.clone())
            .filter(|path| path.starts_with(old))
            .map(|path| path.strip_prefix(old).map(|rest| new.join(rest)).unwrap_or_else(|_| new.to_path_buf()))
            .collect();
        self.remove_tabs_under(old, cx);
        for path in moved {
            let path = if path.as_os_str().is_empty() { new.to_path_buf() } else { path };
            self.open_request(&path, window, cx);
        }
        if let Some(path) = selected_path {
            self.tree_selection.select(&path);
        }
    }

    pub fn duplicate(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let duplicated = self.project.update(cx, |project, _| {
            project.edit_tree(|collection| {
                let request = collection.request(&path).context("the request is gone")?;
                let mut file = request.doc.data.clone();
                file.info.name = format!("{} copy", request.name());
                file.info.seq = None;
                let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
                collection.add_request(&dir, file)
            })
        });
        match duplicated {
            Ok(new_path) => self.open_request(&new_path, window, cx),
            Err(error) => Self::notify_error(window, cx, error),
        }
    }

    pub fn delete(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let what = match self.project.read(cx).collection.item(&path) {
            Some(Item::Folder(folder)) => format!("the folder {} and everything in it", folder.name()),
            Some(Item::Request(request)) => format!("the request {}", request.name()),
            None => return,
        };
        let view = cx.entity().downgrade();
        let description = "Its files are deleted from disk. Git still has them if they were committed.";
        confirm(window, cx, format!("Delete {what}?"), description, "Delete", move |window, cx| {
            let path = path.clone();
            view.update(cx, |view, cx| {
                match view.project.update(cx, |project, _| project.edit_tree(|collection| collection.remove(&path))) {
                    Ok(()) => view.remove_tabs_under(&path, cx),
                    Err(error) => Self::notify_error(window, cx, error),
                }
            })
            .report_unless_gone("delete an item");
        });
    }

    pub fn open_environments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let editor = cx.new(|cx| EnvEditor::new(project, window, cx));
        window.open_dialog(cx, move |dialog, _, _| dialog.title("Environments").w(dp(780.)).child(editor.clone()));
    }
}
