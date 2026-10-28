//! The open collection, shared by the sidebar and every tab: the files, the chosen environment, and
//! the session (script variables, tokens, cookies) that lasts while the app is open.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{AppContext as _, Context, Entity, EventEmitter, Task};
use notify::{RecursiveMode, Watcher as _};
use volley_core::{Collection, Session};

use crate::interact::GuiInteract;
use crate::prefs::Prefs;

pub enum ProjectEvent {
    /// The files were read again (after a save, a git checkout, an editor).
    Reloaded,
    EnvironmentChanged,
}

pub struct Project {
    pub collection: Collection,
    pub env: Option<String>,
    pub session: Session,
    pub interact: GuiInteract,
    /// The last time reading the files failed, shown until they read again.
    pub load_error: Option<String>,
    _watcher: Option<notify::RecommendedWatcher>,
    _watch_task: Option<Task<()>>,
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn open(path: &Path, interact: GuiInteract, cx: &mut gpui::App) -> Result<Entity<Self>> {
        let collection = Collection::open(path)?;
        let prefs = Prefs::load();
        let env = prefs.env_for(&collection.root).filter(|name| collection.environment(name).is_some());
        Prefs::update(|p| p.opened(&collection.root));
        let root = collection.root.clone();
        Ok(cx.new(|cx| {
            let mut project =
                Self { collection, env, session: Session::new(), interact, load_error: None, _watcher: None, _watch_task: None };
            project.watch(root, cx);
            project
        }))
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

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        match self.collection.reload() {
            Ok(()) => self.load_error = None,
            Err(e) => self.load_error = Some(format!("{e:#}")),
        }
        if self.env.as_ref().is_some_and(|name| self.collection.environment(name).is_none()) {
            self.env = None;
        }
        cx.emit(ProjectEvent::Reloaded);
        cx.notify();
    }

    /// Reads the collection again whenever its files change, once they settle.
    fn watch(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let (tx, mut rx) = mpsc::unbounded::<()>();
        let git = root.join(".git");
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            if event.kind.is_access() || event.paths.iter().all(|p| p.starts_with(&git)) {
                return;
            }
            let _ = tx.unbounded_send(());
        });
        let Ok(mut watcher) = watcher else { return };
        if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
            return;
        }
        self._watcher = Some(watcher);
        self._watch_task = Some(cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                // A save or a checkout touches many files at once.
                cx.background_executor().timer(Duration::from_millis(250)).await;
                while rx.try_recv().is_ok() {}
                if this.update(cx, |project, cx| project.reload(cx)).is_err() {
                    break;
                }
            }
        }));
    }
}
