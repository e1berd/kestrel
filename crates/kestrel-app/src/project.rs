use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{AppContext as _, Context, Entity, EventEmitter, Task};
use kestrel_core::format::RequestFile;
use kestrel_core::{Collection, Document, Session};
use notify::{RecursiveMode, Watcher as _};

use kestrel_core::report::report_error;

use crate::collection_tree::TreeIndex;
use crate::interact::GuiInteract;
use crate::prefs::Prefs;
use crate::ui::ReportUnlessGone as _;

pub enum ProjectEvent {
    Reloaded,
    EnvironmentChanged,
}

#[derive(Clone)]
pub struct GitStatus {
    pub branch: String,
    pub changed: usize,
}

pub struct IndexedCollection {
    pub collection: Collection,
    pub tree: Arc<TreeIndex>,
}

impl IndexedCollection {
    pub fn new(collection: Collection) -> Self {
        let tree = Arc::new(TreeIndex::new(&collection.items));
        Self { collection, tree }
    }

    pub fn open(root: &Path) -> anyhow::Result<Self> {
        Ok(Self::new(Collection::open(root)?))
    }
}

pub struct Project {
    pub collection: Collection,
    pub tree: Arc<TreeIndex>,
    pub env: Option<String>,
    pub session: Session,
    pub interact: GuiInteract,
    pub load_error: Option<String>,
    pub git: Option<GitStatus>,
    pub last_sync: Option<SystemTime>,
    pub tree_move_pending: bool,
    reload_pending: bool,
    reload_again: bool,
    reload_generation: u64,
    _watcher: Option<notify::RecommendedWatcher>,
    _watch_task: Option<Task<()>>,
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn from_collection(indexed: IndexedCollection, interact: GuiInteract, cx: &mut gpui::App) -> Entity<Self> {
        let IndexedCollection { collection, tree } = indexed;
        let prefs = Prefs::load();
        let env = prefs.env_for(&collection.root).filter(|name| collection.environment(name).is_some());
        let root = collection.root.clone();
        cx.new(|cx| {
            let mut project = Self {
                collection,
                tree,
                env,
                session: Session::new(),
                interact,
                load_error: None,
                git: None,
                last_sync: None,
                tree_move_pending: false,
                reload_pending: false,
                reload_again: false,
                reload_generation: 0,
                _watcher: None,
                _watch_task: None,
            };
            project.watch(root, cx);
            project.refresh_git(cx);
            project
        })
    }

    pub fn root(&self) -> &Path {
        &self.collection.root
    }

    pub fn set_env(&mut self, env: Option<String>, cx: &mut Context<Self>) {
        self.env = env.clone();
        let root = self.collection.root.clone();
        Prefs::update(|p| p.set_env(&root, env));
        cx.emit(ProjectEvent::EnvironmentChanged);
        cx.notify();
    }

    pub fn edit_collection<T>(&mut self, edit: impl FnOnce(&mut Collection) -> T) -> T {
        self.reload_generation += 1;
        edit(&mut self.collection)
    }

    pub fn edit_tree<T>(&mut self, edit: impl FnOnce(&mut Collection) -> T) -> T {
        let result = self.edit_collection(edit);
        self.tree = Arc::new(TreeIndex::new(&self.collection.items));
        result
    }

    pub fn update_request(&mut self, document: &Document<RequestFile>) {
        let tree_changed = self.collection.request(&document.path).is_none_or(|request| {
            request.doc.data.info.name != document.data.info.name
                || request.doc.data.http.as_ref().map(|http| (&http.method, &http.url))
                    != document.data.http.as_ref().map(|http| (&http.method, &http.url))
        });
        self.edit_collection(|collection| {
            if let Some(request) = collection.request_mut(&document.path) {
                request.doc = document.clone();
            }
        });
        if tree_changed {
            self.tree = Arc::new(TreeIndex::new(&self.collection.items));
        }
    }

    pub fn replace_collection(&mut self, indexed: IndexedCollection, cx: &mut Context<Self>) {
        let previous = std::mem::replace(&mut self.collection, indexed.collection);
        self.tree = indexed.tree;
        cx.background_spawn(async move { drop(previous) }).detach();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if self.tree_move_pending {
            return;
        }
        if self.reload_pending {
            self.reload_again = true;
            return;
        }
        self.reload_pending = true;
        let generation = self.reload_generation;
        let root = self.collection.root.clone();
        let task = cx.background_spawn(async move { IndexedCollection::open(&root) });
        cx.spawn(async move |project, cx| {
            let result = task.await;
            project
                .update(cx, |project, cx| {
                    project.reload_pending = false;
                    if project.reload_again {
                        project.reload_again = false;
                        if let Err(error) = result {
                            report_error("reload the collection", error);
                        }
                        project.reload(cx);
                        return;
                    }
                    if project.tree_move_pending || generation != project.reload_generation {
                        if let Err(error) = result {
                            report_error("reload the collection", error);
                        }
                        return;
                    }
                    match result {
                        Ok(collection) => {
                            project.replace_collection(collection, cx);
                            project.reload_generation += 1;
                            project.load_error = None;
                        }
                        Err(error) => {
                            report_error("reload the collection", &error);
                            project.load_error = Some(format!("{error:#}"));
                        }
                    }
                    if project.env.as_ref().is_some_and(|name| project.collection.environment(name).is_none()) {
                        project.env = None;
                    }
                    project.refresh_git(cx);
                    cx.emit(ProjectEvent::Reloaded);
                    cx.notify();
                })
                .report_unless_gone("reload the collection");
        })
        .detach();
    }

    pub fn finish_tree_move(&mut self, cx: &mut Context<Self>) {
        self.tree_move_pending = false;
        self.reload_generation += 1;
        self.refresh_git(cx);
        cx.emit(ProjectEvent::Reloaded);
        cx.notify();
    }

    fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let root = self.collection.root.clone();
        let task = cx.background_spawn(async move { read_git_status(&root) });
        cx.spawn(async move |project, cx| {
            let git = task.await;
            project
                .update(cx, |project, cx| {
                    project.git = git;
                    cx.notify();
                })
                .report_unless_gone("show the git status");
        })
        .detach();
    }

    fn watch(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let (mut tx, mut rx) = mpsc::channel::<()>(1);
        let task = cx.background_spawn(async move {
            let git = root.join(".git");
            let root_for_events = root.clone();
            let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let event = match event {
                    Ok(event) => event,
                    Err(error) => return report_error("watch the collection", error),
                };
                if event.kind.is_access()
                    || event.paths.iter().all(|path| {
                        path.starts_with(&git)
                            || path.starts_with(root_for_events.join("node_modules"))
                            || (!path.is_dir()
                                && path.extension().is_some_and(|extension| extension != "yml" && extension != "yaml")
                                && path.file_name().is_none_or(|name| name != ".env"))
                    })
                {
                    return;
                }
                if tx.try_send(()).is_err() {
                    return;
                }
            })?;
            watcher.watch(&root, RecursiveMode::Recursive)?;
            anyhow::Ok(watcher)
        });
        self._watch_task = Some(cx.spawn(async move |this, cx| {
            match task.await {
                Ok(watcher) => match this.update(cx, |project, _| project._watcher = Some(watcher)) {
                    Ok(()) => {}
                    Err(error) => {
                        Err::<(), _>(error).report_unless_gone("start watching the collection");
                        return;
                    }
                },
                Err(error) => return report_error("watch the collection", error),
            }
            while rx.next().await.is_some() {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                while rx.try_recv().is_ok() {}
                match this.update(cx, |project, cx| project.reload(cx)) {
                    Ok(()) => {}
                    Err(error) => {
                        Err::<(), _>(error).report_unless_gone("reload a watched collection");
                        break;
                    }
                }
            }
        }));
    }
}

fn read_git_status(root: &Path) -> Option<GitStatus> {
    let run = |args: &[&str]| -> Option<String> {
        match Command::new("git").arg("-C").arg(root).args(args).output() {
            Ok(output) if output.status.success() => Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            Ok(_) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                report_error("read the git status", error);
                None
            }
        }
    };
    let branch = run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let changed = run(&["status", "--porcelain", "--", "."]).map(|out| out.lines().filter(|line| !line.is_empty()).count()).unwrap_or(0);
    Some(GitStatus { branch, changed })
}
