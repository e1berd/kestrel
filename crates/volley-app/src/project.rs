use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{AppContext as _, Context, Entity, EventEmitter, Task};
use notify::{RecursiveMode, Watcher as _};
use volley_core::{Collection, Session};

use volley_core::report::report_error;

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

pub struct Project {
    pub collection: Collection,
    pub env: Option<String>,
    pub session: Session,
    pub interact: GuiInteract,
    pub load_error: Option<String>,
    pub git: Option<GitStatus>,
    pub last_sync: Option<SystemTime>,
    _watcher: Option<notify::RecommendedWatcher>,
    _watch_task: Option<Task<()>>,
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn open(path: &Path, interact: GuiInteract, cx: &mut gpui::App) -> Result<Entity<Self>> {
        let collection = Collection::open(path)?;
        Ok(Self::from_collection(collection, interact, cx))
    }

    pub fn from_collection(collection: Collection, interact: GuiInteract, cx: &mut gpui::App) -> Entity<Self> {
        let prefs = Prefs::load();
        let env = prefs.env_for(&collection.root).filter(|name| collection.environment(name).is_some());
        Prefs::update(|p| p.opened(&collection.root));
        let root = collection.root.clone();
        cx.new(|cx| {
            let mut project = Self {
                collection,
                env,
                session: Session::new(),
                interact,
                load_error: None,
                git: None,
                last_sync: None,
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

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        match self.collection.reload() {
            Ok(()) => self.load_error = None,
            Err(e) => self.load_error = Some(format!("{e:#}")),
        }
        if self.env.as_ref().is_some_and(|name| self.collection.environment(name).is_none()) {
            self.env = None;
        }
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
        let (tx, mut rx) = mpsc::unbounded::<()>();
        let git = root.join(".git");
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let event = match event {
                Ok(event) => event,
                Err(error) => return report_error("watch the collection", error),
            };
            if event.kind.is_access() || event.paths.iter().all(|p| p.starts_with(&git)) {
                return;
            }
            if tx.unbounded_send(()).is_err() {
                return;
            }
        });
        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => return report_error("watch the collection", error),
        };
        if let Err(error) = watcher.watch(&root, RecursiveMode::Recursive) {
            return report_error("watch the collection", error);
        }
        self._watcher = Some(watcher);
        self._watch_task = Some(cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                while rx.try_recv().is_ok() {}
                if this.update(cx, |project, cx| project.reload(cx)).is_err() {
                    break;
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
