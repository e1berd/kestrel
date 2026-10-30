mod command_palette;
mod rail;
mod sign_in;
mod status_bar;
mod title_bar;

pub use title_bar::shortcut;
mod welcome;
mod welcome_screen;

use std::path::{Path, PathBuf};

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, actions, div,
};
use gpui_component::notification::Notification;
use gpui_component::{ActiveTheme as _, TitleBar, WindowExt as _, h_flex, v_flex};

use crate::appearance::{self, dp};
use crate::dialogs::confirm;
use crate::interact::{GuiInteract, PastePrompt};
use crate::prefs::Prefs;
use crate::project::Project;
use crate::project_view::{ProjectView, ProjectViewEvent};
use crate::request_tab::RequestTab;
use crate::settings;
use crate::ui::ReportUnlessGone as _;

actions!(
    kestrel,
    [
        SendRequest,
        SaveRequest,
        CloseTab,
        NewRequest,
        OpenCollection,
        ReloadCollection,
        NextTab,
        PreviousTab,
        NextProject,
        PreviousProject,
        CloseProject,
        OpenSettings,
        OpenCommandPalette
    ]
);

pub struct Workspace {
    views: Vec<Entity<ProjectView>>,
    active: usize,
    interact: GuiInteract,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (prompts, mut prompt_requests) = mpsc::unbounded::<PastePrompt>();
        cx.spawn_in(window, async move |workspace, cx| {
            while let Some(prompt) = prompt_requests.next().await {
                workspace
                    .update_in(cx, |_, window, cx| sign_in::ask_for_redirect(prompt, window, cx))
                    .report_unless_gone("ask how the sign-in ended");
            }
        })
        .detach();
        let subscriptions = vec![cx.observe_window_appearance(window, |_, window, cx| appearance::follow_system_appearance(window, cx))];
        let mut workspace = Self { views: Vec::new(), active: 0, interact: GuiInteract { prompts }, _subscriptions: subscriptions };
        workspace.restore(initial, window, cx);
        workspace
    }

    fn restore(&mut self, initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let prefs = Prefs::load();
        for root in prefs.open_projects.iter().filter(|root| root.join(kestrel_core::collection::COLLECTION_FILE).is_file()) {
            self.open_collection(root, window, cx);
        }
        let focus = initial.clone().or(prefs.active_project);
        if let Some(path) = focus {
            self.open_collection(&path, window, cx);
        }
        if let Some(request) = initial.filter(|path| path.is_file()) {
            self.open_request_file(&request, window, cx);
        }
    }

    fn open_request_file(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(path) = path.canonicalize() else { return };
        self.with_view(cx, |view, cx| view.open_request(&path, window, cx));
    }

    pub fn open_collection(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = kestrel_core::collection::find_root(path) else {
            let message = format!("{} is not a collection: there is no opencollection.yml in it", path.display());
            window.push_notification(Notification::error(message), cx);
            return;
        };
        if let Some(index) = self.views.iter().position(|view| view.read(cx).root(cx) == root) {
            self.activate(index, window, cx);
            return;
        }
        match Project::open(&root, self.interact.clone(), cx) {
            Ok(project) => {
                let view = cx.new(|cx| ProjectView::new(project, window, cx));
                self._subscriptions.push(cx.subscribe(&view, |_, _, _: &ProjectViewEvent, cx| cx.notify()));
                self.views.push(view);
                self.activate(self.views.len() - 1, window, cx);
            }
            Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
        }
    }

    pub fn open_loaded_collection(&mut self, collection: kestrel_core::Collection, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.views.iter().position(|view| view.read(cx).root(cx) == collection.root) {
            self.activate(index, window, cx);
            return;
        }
        let project = Project::from_collection(collection, self.interact.clone(), cx);
        let view = cx.new(|cx| ProjectView::new(project, window, cx));
        self._subscriptions.push(cx.subscribe(&view, |_, _, _: &ProjectViewEvent, cx| cx.notify()));
        self.views.push(view);
        self.activate(self.views.len() - 1, window, cx);
    }

    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.views.get(index) else { return };
        self.active = index;
        let root = view.read(cx).root(cx);
        window.set_window_title(&format!("{} — Kestrel", view.read(cx).name(cx)));
        Prefs::update(|prefs| prefs.opened(&root));
        cx.notify();
    }

    fn close_project(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.views.get(index).cloned() else { return };
        if !view.read(cx).has_unsaved(cx) {
            self.remove_project(&view, window, cx);
            return;
        }
        let workspace = cx.entity().downgrade();
        let title = format!("Close {} with unsaved requests?", view.read(cx).name(cx));
        confirm(window, cx, title, "Their edits will be lost.", "Close", move |window, cx| {
            workspace.update(cx, |workspace, cx| workspace.remove_project(&view, window, cx)).report_unless_gone("close a project");
        });
    }

    fn remove_project(&mut self, view: &Entity<ProjectView>, window: &mut Window, cx: &mut Context<Self>) {
        let root = view.read(cx).root(cx);
        self.views.retain(|v| v != view);
        Prefs::update(|prefs| prefs.closed(&root));
        let next = self.active.min(self.views.len().saturating_sub(1));
        if self.views.is_empty() {
            window.set_window_title("Kestrel");
            cx.notify();
        } else {
            self.activate(next, window, cx);
        }
    }

    fn cycle_project(&mut self, is_forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.views.len();
        if count > 1 {
            let next = if is_forward { (self.active + 1) % count } else { (self.active + count - 1) % count };
            self.activate(next, window, cx);
        }
    }

    fn active_view(&self) -> Option<&Entity<ProjectView>> {
        self.views.get(self.active)
    }

    fn active_request(&self, cx: &gpui::App) -> Option<Entity<RequestTab>> {
        self.active_view()?.read(cx).active_tab().cloned()
    }

    fn with_view(&self, cx: &mut Context<Self>, action: impl FnOnce(&mut ProjectView, &mut Context<ProjectView>)) {
        if let Some(view) = self.active_view().cloned() {
            view.update(cx, action);
        }
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.active_view() {
            Some(view) => view.clone().into_any_element(),
            None => self.render_welcome(cx),
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .key_context("Workspace")
            .on_action(cx.listener(|workspace, _: &SendRequest, window, cx| {
                if let Some(tab) = workspace.active_request(cx) {
                    tab.update(cx, |tab, cx| tab.send(window, cx));
                }
            }))
            .on_action(cx.listener(|workspace, _: &SaveRequest, window, cx| {
                if let Some(tab) = workspace.active_request(cx) {
                    tab.update(cx, |tab, cx| tab.save(window, cx));
                }
            }))
            .on_action(
                cx.listener(|workspace, _: &CloseTab, window, cx| workspace.with_view(cx, |view, cx| view.close_active_tab(window, cx))),
            )
            .on_action(
                cx.listener(|workspace, _: &NewRequest, window, cx| workspace.with_view(cx, |view, cx| view.new_request(None, window, cx))),
            )
            .on_action(cx.listener(|workspace, _: &OpenCollection, window, cx| workspace.prompt_open(window, cx)))
            .on_action(cx.listener(|workspace, _: &ReloadCollection, _, cx| {
                workspace.with_view(cx, |view, cx| view.project.update(cx, |project, cx| project.reload(cx)))
            }))
            .on_action(cx.listener(|workspace, _: &NextTab, _, cx| workspace.with_view(cx, |view, cx| view.cycle_tab(true, cx))))
            .on_action(cx.listener(|workspace, _: &PreviousTab, _, cx| workspace.with_view(cx, |view, cx| view.cycle_tab(false, cx))))
            .on_action(cx.listener(|workspace, _: &NextProject, window, cx| workspace.cycle_project(true, window, cx)))
            .on_action(cx.listener(|workspace, _: &PreviousProject, window, cx| workspace.cycle_project(false, window, cx)))
            .on_action(cx.listener(|workspace, _: &CloseProject, window, cx| workspace.close_project(workspace.active, window, cx)))
            .on_action(cx.listener(|_, _: &OpenSettings, window, cx| settings::open(window, cx)))
            .on_action(cx.listener(|workspace, _: &OpenCommandPalette, window, cx| command_palette::open(workspace, window, cx)))
            .bg(appearance::palette(cx).surface)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(self.render_title(cx)))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .pr(dp(8.))
                    .child(self.render_rail(cx))
                    .child(div().flex_1().min_w_0().h_full().child(self.render_content(cx))),
            )
            .child(self.render_status_bar(cx))
    }
}
