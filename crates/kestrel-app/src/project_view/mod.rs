mod dragging;
mod empty_state;
mod openapi_sync;
mod sidebar;
mod sidebar_state;
mod tab_dragging;
mod tab_strip;
mod tree_actions;
mod tree_hover;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    Task, Window, div,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{ResizableState, WindowExt as _, h_resizable, resizable_panel, v_flex};

use crate::appearance::{self, dp};
use crate::collection_tree::SidebarTree;
use crate::dialogs::confirm;
use crate::project::{Project, ProjectEvent};
use crate::request_tab::{RequestTab, TabEvent};
use crate::ui::ReportUnlessGone as _;

pub enum ProjectViewEvent {
    Changed,
}

pub struct ProjectView {
    pub project: Entity<Project>,
    tabs: Vec<Entity<RequestTab>>,
    active: usize,
    expanded: HashSet<PathBuf>,
    filter: Entity<InputState>,
    layout: Entity<ResizableState>,
    tree_scroll: gpui::UniformListScrollHandle,
    sidebar: SidebarTree,
    filter_task: Option<Task<()>>,
    tree_hover: Option<(PathBuf, Task<()>)>,
    filtering: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProjectViewEvent> for ProjectView {}

impl ProjectView {
    pub fn new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter requests"));
        let sidebar = SidebarTree::new(project.read(cx).tree.clone(), &HashSet::new());
        let subscriptions = vec![
            cx.subscribe(&filter, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.schedule_filter(cx);
                }
            }),
            cx.subscribe_in(&project, window, |view, _, event: &ProjectEvent, _, cx| {
                if matches!(event, ProjectEvent::Reloaded) {
                    view.drop_missing_tabs(cx);
                }
                cx.emit(ProjectViewEvent::Changed);
                cx.notify();
            }),
        ];
        Self {
            project,
            tabs: Vec::new(),
            active: 0,
            expanded: HashSet::new(),
            filter,
            layout: cx.new(|_| ResizableState::default()),
            tree_scroll: Default::default(),
            sidebar,
            filter_task: None,
            tree_hover: None,
            filtering: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn name(&self, cx: &App) -> String {
        self.project.read(cx).collection.name()
    }

    pub fn root(&self, cx: &App) -> PathBuf {
        self.project.read(cx).root().to_path_buf()
    }

    pub fn has_unsaved(&self, cx: &App) -> bool {
        self.tabs.iter().any(|tab| tab.read(cx).dirty)
    }

    pub fn active_tab(&self) -> Option<&Entity<RequestTab>> {
        self.tabs.get(self.active)
    }

    pub fn open_request(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.read(cx).path == path) {
            self.reveal_request(path, window, cx);
            self.active = index;
            cx.notify();
            return;
        }
        if self.project.read(cx).collection.request(path).is_none() {
            return;
        }
        self.reveal_request(path, window, cx);
        let project = self.project.clone();
        let path = path.to_path_buf();
        let tab = cx.new(|cx| RequestTab::new(project, path, window, cx).expect("the request is in the collection"));
        self._subscriptions.push(cx.subscribe(&tab, |_, _, _: &TabEvent, cx| {
            cx.emit(ProjectViewEvent::Changed);
            cx.notify();
        }));
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        cx.notify();
    }

    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index).cloned() else { return };
        if !tab.read(cx).dirty {
            self.remove_tab(&tab, cx);
            return;
        }
        let view = cx.entity().downgrade();
        let title = format!("Close {} without saving?", tab.read(cx).title());
        confirm(window, cx, title, "The edits that are not saved will be lost.", "Close", move |_, cx| {
            view.update(cx, |view, cx| view.remove_tab(&tab, cx)).report_unless_gone("close a tab");
        });
    }

    pub fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.active, window, cx);
    }

    pub fn cycle_tab(&mut self, is_forward: bool, cx: &mut Context<Self>) {
        let count = self.tabs.len();
        if count > 0 {
            self.active = if is_forward { (self.active + 1) % count } else { (self.active + count - 1) % count };
            cx.notify();
        }
    }

    fn expand_ancestors(&mut self, path: &Path, cx: &App) {
        let folders: Vec<PathBuf> = self.project.read(cx).collection.ancestors(path).iter().map(|folder| folder.dir.clone()).collect();
        self.expanded.extend(folders);
        if !self.sidebar.filtered {
            self.sidebar.show_expanded(&self.expanded);
        }
    }

    fn activate_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.active = index;
        cx.notify();
    }

    fn remove_tab(&mut self, tab: &Entity<RequestTab>, cx: &mut Context<Self>) {
        self.tabs.retain(|t| t != tab);
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        cx.emit(ProjectViewEvent::Changed);
        cx.notify();
    }

    fn remove_tabs_under(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.tabs.retain(|tab| !tab.read(cx).path.starts_with(path));
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        cx.notify();
    }

    fn drop_missing_tabs(&mut self, cx: &mut Context<Self>) {
        let collection = &self.project.read(cx).collection;
        let keep: Vec<Entity<RequestTab>> =
            self.tabs.iter().filter(|tab| collection.request(&tab.read(cx).path).is_some() || tab.read(cx).dirty).cloned().collect();
        self.tabs = keep;
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
    }

    fn notify_error(window: &mut Window, cx: &mut App, error: anyhow::Error) {
        kestrel_core::report::report_error("edit the collection", &error);
        window.push_notification(gpui_component::notification::Notification::error(format!("{error:#}")), cx);
    }

    fn render_editor_area(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let editor: AnyElement = match self.active_tab() {
            Some(tab) => tab.clone().into_any_element(),
            None => empty_state::render_empty_editor(cx).into_any_element(),
        };
        div()
            .size_full()
            .pl(dp(4.))
            .pb(dp(2.))
            .child(
                v_flex()
                    .size_full()
                    .rounded(dp(14.))
                    .overflow_hidden()
                    .bg(palette.background)
                    .border_1()
                    .border_color(palette.border_soft)
                    .shadow(appearance::elevation::card(cx))
                    .when(!self.tabs.is_empty(), |el| el.child(self.render_tab_strip(cx)))
                    .child(div().flex_1().min_h_0().child(editor)),
            )
            .into_any_element()
    }
}

impl Render for ProjectView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_sidebar(cx);
        h_resizable("project")
            .with_state(&self.layout)
            .child(resizable_panel().size(dp(276.)).size_range(dp(180.)..dp(640.)).child(self.render_sidebar(cx)))
            .child(resizable_panel().child(self.render_editor_area(cx)))
    }
}
