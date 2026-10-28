//! The window's content: the title bar with the environment, the collection tree on the left, the
//! open requests on the right, or the welcome screen when no collection is open.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, ElementId, Entity, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
    actions, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use gpui_component::notification::Notification;
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _, Theme, ThemeMode, TitleBar, WindowExt as _, h_flex, h_resizable, resizable_panel, v_flex,
};
use volley_core::format::RequestFile;
use volley_core::{Collection, Item};
use volley_openapi::{GroupBy, SyncOptions};

use crate::env_editor::EnvEditor;
use crate::interact::{GuiInteract, PastePrompt};
use crate::prefs::Prefs;
use crate::project::{Project, ProjectEvent};
use crate::request_tab::{RequestTab, TabEvent};
use crate::style;

actions!(kestrel, [SendRequest, SaveRequest, CloseTab, NewRequest, OpenCollection, ReloadCollection, ToggleTheme, NextTab, PreviousTab]);

pub struct Workspace {
    project: Option<Entity<Project>>,
    tabs: Vec<Entity<RequestTab>>,
    active: usize,
    expanded: HashSet<PathBuf>,
    filter: Entity<InputState>,
    interact: GuiInteract,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (prompts, mut rx) = mpsc::unbounded::<PastePrompt>();
        cx.spawn_in(window, async move |this, cx| {
            while let Some(prompt) = rx.next().await {
                if this.update_in(cx, |this, window, cx| this.ask_for_redirect(prompt, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter requests"));
        let subscriptions = vec![cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify()
            }
        })];
        let mut workspace = Self {
            project: None,
            tabs: Vec::new(),
            active: 0,
            expanded: HashSet::new(),
            filter,
            interact: GuiInteract { prompts },
            _subscriptions: subscriptions,
        };
        let initial =
            initial.or_else(|| Prefs::load().recent.into_iter().find(|p| p.join(volley_core::collection::COLLECTION_FILE).is_file()));
        if let Some(path) = initial {
            workspace.open_collection(&path, window, cx);
        }
        workspace
    }

    pub fn open_collection(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match Project::open(path, self.interact.clone(), cx) {
            Ok(project) => {
                self.tabs.clear();
                self.active = 0;
                self.expanded.clear();
                self._subscriptions.truncate(1);
                self._subscriptions.push(cx.subscribe_in(&project, window, |this, _, event: &ProjectEvent, _, cx| {
                    if matches!(event, ProjectEvent::Reloaded) {
                        this.drop_missing_tabs(cx);
                    }
                    cx.notify();
                }));
                let title = format!("{} — Kestrel", project.read(cx).collection.name());
                window.set_window_title(&title);
                self.project = Some(project);
            }
            Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
        }
        cx.notify();
    }

    fn drop_missing_tabs(&mut self, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        let collection = &project.read(cx).collection;
        let keep: Vec<bool> = self.tabs.iter().map(|tab| collection.request(&tab.read(cx).path).is_some() || tab.read(cx).dirty).collect();
        let mut index = 0;
        self.tabs.retain(|_| {
            index += 1;
            keep[index - 1]
        });
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
    }

    fn prompt_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open collection".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| this.open_collection(&path, window, cx));
                }
            }
        })
        .detach();
    }

    fn prompt_new_collection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Make the collection in this folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(dir) = paths.into_iter().next() else { return };
            let _ = this.update_in(cx, |this, window, cx| {
                let name = volley_core::collection::file_stem(&dir);
                match Collection::create(&dir, &name) {
                    Ok(_) => this.open_collection(&dir, window, cx),
                    Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
                }
            });
        })
        .detach();
    }

    /// Picks a spec, then the folder for the new collection.
    fn prompt_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let spec = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("OpenAPI spec (JSON or YAML)".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = spec.await else { return };
            let Some(spec_path) = paths.into_iter().next() else { return };
            let Ok(dir) = this.update(cx, |_, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Make the collection in this folder".into()),
                })
            }) else {
                return;
            };
            let Ok(Ok(Some(dirs))) = dir.await else { return };
            let Some(dir) = dirs.into_iter().next() else { return };
            let result = cx
                .background_spawn(async move {
                    let spec = volley_openapi::load(&spec_path.display().to_string(), Path::new("/"))?;
                    let source = relative(&spec_path, &dir).unwrap_or_else(|| spec_path.display().to_string());
                    volley_openapi::import(&dir, &source, &spec, GroupBy::Tags, None).map(|(c, report)| (c.root, report))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok((root, report)) => {
                    this.open_collection(&root, window, cx);
                    window.push_notification(Notification::success(format!("Imported: {report}")), cx);
                }
                Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
            });
        })
        .detach();
    }

    /// Sync from the spec the collection names; asks for one when it names none.
    fn sync_openapi(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let collection = project.read(cx).collection.clone();
        let Some((source, group_by)) = collection.doc.data.openapi_sources().into_iter().next() else {
            self.ask_for_spec(window, cx);
            return;
        };
        window.push_notification(Notification::info(format!("Syncing from {source}…")), cx);
        let task = cx.background_spawn(async move {
            let mut collection = collection;
            let spec = volley_openapi::load(&source, &collection.root)?;
            volley_openapi::sync(&mut collection, &spec, &SyncOptions { group_by: GroupBy::parse(&group_by), prune: false, dry_run: false })
        });
        cx.spawn_in(window, async move |_, cx| {
            let result = task.await;
            let _ = project.update_in(cx, |project, window, cx| {
                match result {
                    Ok(report) => {
                        let mut text = format!("Synced: {report}");
                        if !report.stale.is_empty() {
                            text.push_str(&format!(". Not in the spec any more: {}", report.stale.join(", ")));
                        }
                        window.push_notification(Notification::success(text), cx);
                    }
                    Err(e) => window.push_notification(Notification::error(format!("Sync failed: {e:#}")), cx),
                }
                project.reload(cx);
            });
        })
        .detach();
    }

    fn ask_for_spec(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("../backend/openapi.json or https://…/openapi.json"));
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let input = input.clone();
            let project = project.clone();
            let this = this.clone();
            dialog
                .title("Sync from OpenAPI")
                .w(px(560.))
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(
                            "Where is the spec? A path relative to the collection, or a URL. Kestrel remembers it in opencollection.yml.",
                        ))
                        .child(Input::new(&input)),
                )
                .child(h_flex().justify_end().gap_2().pt_2().child(Button::new("sync-go").primary().label("Sync").on_click(
                    move |_, window, cx| {
                        let source = input.read(cx).value().trim().to_string();
                        if source.is_empty() {
                            return;
                        }
                        project.update(cx, |project, _| remember_spec(&mut project.collection, &source));
                        window.close_dialog(cx);
                        let _ = this.update(cx, |this, cx| this.sync_openapi(window, cx));
                    },
                )))
        });
    }

    fn ask_for_redirect(&mut self, prompt: PastePrompt, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(format!("{}?code=…", prompt.callback)));
        let reply = std::rc::Rc::new(std::cell::RefCell::new(Some(prompt.reply)));
        window.open_dialog(cx, move |dialog, _, cx| {
            let input = input.clone();
            let reply = reply.clone();
            dialog
                .title("Finish signing in")
                .w(px(620.))
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(
                            "The browser opened the sign-in page. The callback URL is not on this machine, so once you have signed in, copy the address the browser ended on and paste it here. (Or add http://127.0.0.1:<port>/callback to the client's redirect URIs and use it as the callback: then Kestrel catches it by itself.)",
                        ))
                        .child(Input::new(&input)),
                )
                .child(h_flex().justify_end().pt_2().child(Button::new("redirect-go").primary().label("Continue").on_click(move |_, window, cx| {
                    if let Some(reply) = reply.borrow_mut().take() {
                        let _ = reply.send(input.read(cx).value().trim().to_string());
                    }
                    window.close_dialog(cx);
                })))
        });
    }

    fn open_request(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|t| t.read(cx).path == path) {
            self.active = index;
            cx.notify();
            return;
        }
        let Some(project) = self.project.clone() else { return };
        if project.read(cx).collection.request(path).is_none() {
            return;
        }
        let path = path.to_path_buf();
        let tab = cx.new(|cx| RequestTab::new(project, path, window, cx).expect("the request is in the collection"));
        self._subscriptions.push(cx.subscribe(&tab, Self::on_tab_event));
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index).cloned() else { return };
        if tab.read(cx).dirty {
            let this = cx.entity().downgrade();
            let name = tab.read(cx).title(cx);
            window.open_alert_dialog(cx, move |alert, _, _| {
                let this = this.clone();
                let tab = tab.clone();
                alert
                    .title(format!("Close {name} without saving?"))
                    .description("The edits that are not saved will be lost.")
                    .show_cancel(true)
                    .ok_text("Close")
                    .on_ok(move |_, _, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.tabs.retain(|t| t != &tab);
                            this.active = this.active.min(this.tabs.len().saturating_sub(1));
                            cx.notify();
                        });
                        true
                    })
            });
            return;
        }
        self.tabs.remove(index);
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        cx.notify();
    }

    fn active_tab(&self) -> Option<&Entity<RequestTab>> {
        self.tabs.get(self.active)
    }

    fn new_request(&mut self, dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let dir = dir.unwrap_or_else(|| project.read(cx).root().to_path_buf());
        self.ask_name("New request", "Request name", String::new(), window, cx, move |this, name, window, cx| {
            let result = project.update(cx, |project, cx| {
                let result = project.collection.add_request(&dir, RequestFile::new_http(&name, "GET", "{{baseUrl}}/"));
                cx.notify();
                result
            });
            match result {
                Ok(path) => {
                    this.expanded.insert(dir.clone());
                    this.open_request(&path, window, cx);
                }
                Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
            }
        });
    }

    fn new_folder(&mut self, parent: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let parent = parent.unwrap_or_else(|| project.read(cx).root().to_path_buf());
        self.ask_name("New folder", "Folder name", String::new(), window, cx, move |this, name, window, cx| {
            let result = project.update(cx, |project, cx| {
                let result = project.collection.add_folder(&parent, &name);
                cx.notify();
                result
            });
            match result {
                Ok(dir) => {
                    this.expanded.insert(parent.clone());
                    this.expanded.insert(dir);
                }
                Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
            }
        });
    }

    fn rename(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let Some(current) = project.read(cx).collection.item(&path).map(Item::name) else { return };
        self.ask_name("Rename", "Name", current, window, cx, move |this, name, window, cx| {
            let result = project.update(cx, |project, cx| {
                let result = project.collection.rename(&path, &name);
                cx.notify();
                result
            });
            match result {
                Ok(new_path) => {
                    let mut reopen = Vec::new();
                    for tab in &this.tabs {
                        let old = tab.read(cx).path.clone();
                        if old.starts_with(&path) {
                            reopen.push(new_path.join(old.strip_prefix(&path).unwrap_or(Path::new(""))));
                        }
                    }
                    this.tabs.retain(|tab| !tab.read(cx).path.starts_with(&path));
                    for p in reopen {
                        let p = if p.as_os_str().is_empty() || p == new_path.join("") { new_path.clone() } else { p };
                        this.open_request(&p, window, cx);
                    }
                }
                Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
            }
        });
    }

    fn duplicate(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let result = project.update(cx, |project, cx| {
            let Some(request) = project.collection.request(&path) else { return Err(anyhow::anyhow!("no such request")) };
            let mut file = request.doc.data.clone();
            file.info.name = format!("{} copy", request.name());
            file.info.seq = None;
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let result = project.collection.add_request(&dir, file);
            cx.notify();
            result
        });
        match result {
            Ok(new_path) => self.open_request(&new_path, window, cx),
            Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
        }
    }

    fn delete(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let Some(item) = project.read(cx).collection.item(&path) else { return };
        let what = match item {
            Item::Folder(f) => format!("the folder {} and everything in it", f.name()),
            Item::Request(r) => format!("the request {}", r.name()),
        };
        let this = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let project = project.clone();
            let path = path.clone();
            let this = this.clone();
            alert
                .title(format!("Delete {what}?"))
                .description("Its files are deleted from disk. Git still has them if they were committed.")
                .show_cancel(true)
                .ok_text("Delete")
                .on_ok(move |_, window, cx| {
                    let result = project.update(cx, |project, cx| {
                        let result = project.collection.remove(&path);
                        cx.notify();
                        result
                    });
                    match result {
                        Ok(()) => {
                            let _ = this.update(cx, |this, cx| {
                                this.tabs.retain(|tab| !tab.read(cx).path.starts_with(&path));
                                this.active = this.active.min(this.tabs.len().saturating_sub(1));
                                cx.notify();
                            });
                        }
                        Err(e) => window.push_notification(Notification::error(format!("{e:#}")), cx),
                    }
                    true
                })
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn ask_name(
        &mut self,
        title: &'static str,
        placeholder: &'static str,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
        done: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value));
        input.update(cx, |input, cx| input.focus(window, cx));
        let this = cx.entity().downgrade();
        let done = std::rc::Rc::new(done);
        let submit = {
            let input = input.clone();
            let this = this.clone();
            let done = done.clone();
            std::rc::Rc::new(move |window: &mut Window, cx: &mut App| {
                let name = input.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return;
                }
                window.close_dialog(cx);
                let done = done.clone();
                let _ = this.update(cx, |this, cx| done(this, name, window, cx));
            })
        };
        let enter = submit.clone();
        let subscription = cx.subscribe_in(&input, window, move |_, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                enter(window, cx);
            }
        });
        self._subscriptions.push(subscription);
        window.open_dialog(cx, move |dialog, _, _| {
            let submit = submit.clone();
            dialog.title(title).w(px(420.)).child(Input::new(&input)).child(
                h_flex()
                    .justify_end()
                    .pt_2()
                    .child(Button::new("name-ok").primary().label("OK").on_click(move |_, window, cx| submit(window, cx))),
            )
        });
    }

    fn open_environments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else { return };
        let editor = cx.new(|cx| EnvEditor::new(project, window, cx));
        window.open_dialog(cx, move |dialog, _, _| dialog.title("Environments").w(px(760.)).child(editor.clone()));
    }

    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dark = !cx.theme().mode.is_dark();
        Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
        Prefs::update(|p| p.dark = Some(dark));
        cx.notify();
    }

    fn on_tab_event(&mut self, _: Entity<RequestTab>, _: &TabEvent, cx: &mut Context<Self>) {
        cx.notify();
    }
}

/// Writes `extensions.volley.openapi` into the collection's file.
fn remember_spec(collection: &mut Collection, source: &str) {
    use serde_yaml_ng::{Mapping, Value};
    let extensions = collection.doc.data.extra.entry("extensions".into()).or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(extensions) = extensions.as_mapping_mut() else { return };
    let volley = extensions.entry("volley".into()).or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(volley) = volley.as_mapping_mut() else { return };
    let mut entry = Mapping::new();
    entry.insert("sourceUrl".into(), source.into());
    entry.insert("groupBy".into(), "tags".into());
    volley.insert("openapi".into(), Value::Sequence(vec![Value::Mapping(entry)]));
    let _ = collection.doc.save();
}

fn relative(path: &Path, base: &Path) -> Option<String> {
    let path: Vec<_> = path.components().collect();
    let base: Vec<_> = base.components().collect();
    let common = path.iter().zip(&base).take_while(|(a, b)| a == b).count();
    if common <= 1 {
        return None;
    }
    let mut out = PathBuf::new();
    for _ in common..base.len() {
        out.push("..");
    }
    for part in &path[common..] {
        out.push(part);
    }
    Some(out.display().to_string())
}

// --- The tree ----------------------------------------------------------------------------------

impl Workspace {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(project) = &self.project else { return div().into_any_element() };
        let project = project.read(cx);
        let theme = cx.theme();
        let filter = self.filter.read(cx).value().to_lowercase();
        let mut rows = Vec::new();
        self.tree_rows(&project.collection.items, 0, &filter, &mut rows, cx);
        let active_path = self.active_tab().map(|t| t.read(cx).path.clone());
        let header = h_flex()
            .px_2()
            .py_1()
            .gap_1()
            .items_center()
            .child(div().flex_1().min_w_0().truncate().text_sm().font_weight(FontWeight::SEMIBOLD).child(project.collection.name()))
            .child(
                Button::new("add-request")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip("New request")
                    .on_click(cx.listener(|this, _, window, cx| this.new_request(None, window, cx))),
            )
            .child(
                Button::new("add-folder")
                    .xsmall()
                    .ghost()
                    .icon(IconName::FolderClosed)
                    .tooltip("New folder")
                    .on_click(cx.listener(|this, _, window, cx| this.new_folder(None, window, cx))),
            )
            .child(
                Button::new("sync")
                    .xsmall()
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .tooltip("Sync from OpenAPI")
                    .on_click(cx.listener(|this, _, window, cx| this.sync_openapi(window, cx))),
            )
            .child(Button::new("more").xsmall().ghost().icon(IconName::Ellipsis).dropdown_menu({
                let this = cx.entity().downgrade();
                move |menu, _, _| {
                    let item = |label: &'static str, f: fn(&mut Workspace, &mut Window, &mut Context<Workspace>)| {
                        let this = this.clone();
                        PopupMenuItem::new(label).on_click(move |_, window, cx| {
                            let _ = this.update(cx, |this, cx| f(this, window, cx));
                        })
                    };
                    menu.item(item("Environments…", |this, window, cx| this.open_environments(window, cx)))
                        .item(item("Reload from disk", |this, _, cx| {
                            if let Some(p) = &this.project {
                                p.update(cx, |p, cx| p.reload(cx));
                            }
                        }))
                        .item(item("Show in file manager", |this, _, cx| {
                            if let Some(p) = &this.project {
                                cx.reveal_path(&p.read(cx).root().join(volley_core::collection::COLLECTION_FILE));
                            }
                        }))
                        .separator()
                        .item(item("Open collection…", |this, window, cx| this.prompt_open(window, cx)))
                        .item(item("New collection…", |this, window, cx| this.prompt_new_collection(window, cx)))
                        .item(item("Import OpenAPI…", |this, window, cx| this.prompt_import(window, cx)))
                }
            }));
        let error = project.load_error.clone();
        v_flex()
            .size_full()
            .bg(theme.sidebar)
            .child(header)
            .when_some(error, |el, error| el.child(div().mx_2().mb_1().text_xs().text_color(theme.red).child(error)))
            .child(div().px_2().pb_2().child(Input::new(&self.filter).small().cleanable(true)))
            .child(
                div()
                    .id("tree")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .pb_4()
                    .children(rows.into_iter().map(|row| self.render_row(row, active_path.as_deref(), cx))),
            )
            .into_any_element()
    }

    fn tree_rows(&self, items: &[Item], depth: usize, filter: &str, out: &mut Vec<Row>, cx: &App) {
        for item in items {
            match item {
                Item::Folder(folder) => {
                    let mut inner = Vec::new();
                    let open = !filter.is_empty() || self.expanded.contains(&folder.dir);
                    if open {
                        self.tree_rows(&folder.children, depth + 1, filter, &mut inner, cx);
                    }
                    if !filter.is_empty() && inner.is_empty() && !folder.name().to_lowercase().contains(filter) {
                        continue;
                    }
                    out.push(Row { path: folder.dir.clone(), name: folder.name(), method: None, depth, open });
                    out.extend(inner);
                }
                Item::Request(request) => {
                    let name = request.name();
                    let url = request.doc.data.http.as_ref().map(|h| h.url.to_lowercase()).unwrap_or_default();
                    if !filter.is_empty() && !name.to_lowercase().contains(filter) && !url.contains(filter) {
                        continue;
                    }
                    out.push(Row { path: request.doc.path.clone(), name, method: Some(request.method()), depth, open: false });
                }
            }
        }
    }

    fn render_row(&self, row: Row, active: Option<&Path>, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let is_active = active == Some(row.path.as_path());
        let path = row.path.clone();
        let id = ElementId::Name(SharedString::from(format!("row-{}", row.path.display())));
        let is_folder = row.method.is_none();
        let base = h_flex()
            .id(id)
            .w_full()
            .h(px(26.))
            .pl(px(8. + row.depth as f32 * 14.))
            .pr_2()
            .gap_1()
            .items_center()
            .text_sm()
            .cursor_pointer()
            .rounded_sm()
            .when(is_active, |el| el.bg(theme.list_active))
            .hover(|el| el.bg(theme.sidebar_accent))
            .child(match &row.method {
                None => h_flex()
                    .gap_1()
                    .child(
                        gpui_component::Icon::new(if row.open { IconName::ChevronDown } else { IconName::ChevronRight })
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        gpui_component::Icon::new(if row.open { IconName::FolderOpen } else { IconName::FolderClosed })
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    )
                    .into_any_element(),
                Some(method) => style::method_label(method, cx).into_any_element(),
            })
            .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
            .on_click(cx.listener({
                let path = path.clone();
                move |this, event: &ClickEvent, window, cx| {
                    if is_folder {
                        if !this.expanded.remove(&path) {
                            this.expanded.insert(path.clone());
                        }
                        cx.notify();
                    } else {
                        let _ = event;
                        this.open_request(&path, window, cx);
                    }
                }
            }));
        let this = cx.entity().downgrade();
        base.context_menu(move |menu, _, _| {
            let action = |label: &'static str, f: fn(&mut Workspace, PathBuf, &mut Window, &mut Context<Workspace>)| {
                let this = this.clone();
                let path = path.clone();
                PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let path = path.clone();
                    let _ = this.update(cx, |this, cx| f(this, path, window, cx));
                })
            };
            let menu = if is_folder {
                menu.item(action("New request", |this, p, window, cx| this.new_request(Some(p), window, cx)))
                    .item(action("New folder", |this, p, window, cx| this.new_folder(Some(p), window, cx)))
                    .separator()
            } else {
                menu.item(action("Open", |this, p, window, cx| this.open_request(&p, window, cx)))
                    .item(action("Duplicate", |this, p, window, cx| this.duplicate(p, window, cx)))
            };
            menu.item(action("Rename…", |this, p, window, cx| this.rename(p, window, cx)))
                .item(action("Show in file manager", |_, p, _, cx| cx.reveal_path(&p)))
                .separator()
                .item(action("Delete…", |this, p, window, cx| this.delete(p, window, cx)))
        })
        .into_any_element()
    }
}

struct Row {
    path: PathBuf,
    name: String,
    method: Option<String>,
    depth: usize,
    open: bool,
}

// --- Tabs, title bar, welcome --------------------------------------------------------------------

impl Workspace {
    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex().id("tabs").h(px(34.)).flex_none().overflow_x_scroll().border_b_1().border_color(theme.border).bg(theme.tab_bar).children(
            self.tabs.iter().enumerate().map(|(index, tab)| {
                let tab = tab.read(cx);
                let active = index == self.active;
                h_flex()
                    .id(("tab", index))
                    .h_full()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .flex_none()
                    .max_w(px(240.))
                    .border_r_1()
                    .border_color(theme.border)
                    .text_sm()
                    .cursor_pointer()
                    .when(active, |el| el.bg(theme.tab_active).text_color(theme.tab_active_foreground))
                    .when(!active, |el| el.text_color(theme.tab_foreground))
                    .child(style::method_label(tab.method(), cx))
                    .child(div().min_w_0().truncate().child(tab.title(cx)))
                    .child(
                        div()
                            .id(("close", index))
                            .w(px(14.))
                            .flex_none()
                            .child(if tab.dirty {
                                div().size(px(8.)).rounded_full().bg(theme.foreground.opacity(0.6)).into_any_element()
                            } else {
                                gpui_component::Icon::new(IconName::Close).xsmall().into_any_element()
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_tab(index, window, cx);
                            })),
                    )
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| this.close_tab(index, window, cx)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = index;
                        cx.notify();
                    }))
            }),
        )
    }

    fn render_env_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(project) = self.project.clone() else { return div().into_any_element() };
        let read = project.read(cx);
        let current = read.env.clone();
        let names: Vec<String> = read.collection.environments.iter().map(|e| e.name()).collect();
        let label = current.clone().unwrap_or_else(|| "No environment".into());
        let this = cx.entity().downgrade();
        Button::new("env")
            .small()
            .ghost()
            .label(label)
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                let pick = |name: Option<String>, label: String, checked: bool| {
                    let project = project.clone();
                    PopupMenuItem::new(label).checked(checked).on_click(move |_, _, cx| {
                        let name = name.clone();
                        project.update(cx, |p, cx| p.set_env(name, cx));
                    })
                };
                let mut menu = menu.item(pick(None, "No environment".into(), current.is_none()));
                for name in &names {
                    menu = menu.item(pick(Some(name.clone()), name.clone(), current.as_deref() == Some(name)));
                }
                let this = this.clone();
                menu.separator().item(PopupMenuItem::new("Edit environments…").on_click(move |_, window, cx| {
                    let _ = this.update(cx, |this, cx| this.open_environments(window, cx));
                }))
            })
            .into_any_element()
    }

    fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let recent: Vec<PathBuf> =
            Prefs::load().recent.into_iter().filter(|p| p.join(volley_core::collection::COLLECTION_FILE).is_file()).collect();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(div().text_2xl().font_weight(FontWeight::BOLD).child("Kestrel"))
            .child(div().text_sm().text_color(theme.muted_foreground).child("A free API client that keeps your requests as files in git."))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("w-open")
                            .primary()
                            .icon(IconName::FolderOpen)
                            .label("Open collection")
                            .on_click(cx.listener(|this, _, window, cx| this.prompt_open(window, cx))),
                    )
                    .child(
                        Button::new("w-new")
                            .icon(IconName::Plus)
                            .label("New collection")
                            .on_click(cx.listener(|this, _, window, cx| this.prompt_new_collection(window, cx))),
                    )
                    .child(
                        Button::new("w-import")
                            .icon(IconName::FileText)
                            .label("Import OpenAPI")
                            .on_click(cx.listener(|this, _, window, cx| this.prompt_import(window, cx))),
                    ),
            )
            .when(!recent.is_empty(), |el| {
                el.child(
                    v_flex().w(px(520.)).gap_1().pt_4().child(div().text_xs().text_color(theme.muted_foreground).child("Recent")).children(
                        recent.into_iter().enumerate().map(|(i, path)| {
                            let shown = path.display().to_string();
                            h_flex()
                                .id(("recent", i))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|el| el.bg(cx.theme().accent))
                                .text_sm()
                                .child(shown)
                                .on_click(cx.listener(move |this, _, window, cx| this.open_collection(&path, window, cx)))
                        }),
                    ),
                )
            })
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = window;
        let is_dark = cx.theme().mode.is_dark();
        let title = h_flex()
            .w_full()
            .pr_2()
            .gap_2()
            .items_center()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Kestrel"))
            .child(div().flex_1())
            .child(self.render_env_picker(cx))
            .child(
                Button::new("theme")
                    .small()
                    .ghost()
                    .icon(if is_dark { IconName::Sun } else { IconName::Moon })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx))),
            );
        let body: AnyElement = if self.project.is_none() {
            self.render_welcome(cx)
        } else {
            let editor: AnyElement = match self.active_tab() {
                Some(tab) => tab.clone().into_any_element(),
                None => v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .text_color(cx.theme().muted_foreground)
                    .child("Pick a request on the left, or make one with +")
                    .child(div().text_xs().child("Ctrl+Enter sends · Ctrl+S saves · Ctrl+W closes · Ctrl+N new request"))
                    .into_any_element(),
            };
            h_resizable("workspace")
                .child(resizable_panel().size(px(300.)).size_range(px(180.)..px(600.)).child(self.render_sidebar(cx)))
                .child(
                    resizable_panel().child(
                        v_flex()
                            .size_full()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .when(!self.tabs.is_empty(), |el| el.child(self.render_tabs(cx)))
                            .child(div().flex_1().min_h_0().child(editor)),
                    ),
                )
                .into_any_element()
        };
        v_flex()
            .size_full()
            .key_context("Workspace")
            .on_action(cx.listener(|this, _: &SendRequest, window, cx| {
                if let Some(tab) = this.active_tab().cloned() {
                    tab.update(cx, |tab, cx| tab.send(window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &SaveRequest, window, cx| {
                if let Some(tab) = this.active_tab().cloned() {
                    tab.update(cx, |tab, cx| tab.save(window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| this.close_tab(this.active, window, cx)))
            .on_action(cx.listener(|this, _: &NewRequest, window, cx| this.new_request(None, window, cx)))
            .on_action(cx.listener(|this, _: &OpenCollection, window, cx| this.prompt_open(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleTheme, window, cx| this.toggle_theme(window, cx)))
            .on_action(cx.listener(|this, _: &ReloadCollection, _, cx| {
                if let Some(p) = &this.project {
                    p.update(cx, |p, cx| p.reload(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| {
                if !this.tabs.is_empty() {
                    this.active = (this.active + 1) % this.tabs.len();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| {
                if !this.tabs.is_empty() {
                    this.active = (this.active + this.tabs.len() - 1) % this.tabs.len();
                    cx.notify();
                }
            }))
            .bg(cx.theme().background)
            .child(TitleBar::new().child(title))
            .child(div().flex_1().min_h_0().child(body))
    }
}
