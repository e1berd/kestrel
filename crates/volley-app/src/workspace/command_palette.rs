use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div, rems,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit_assets::IconName;

use super::Workspace;
use crate::appearance::{self, dp};
use crate::prefs::ThemeChoice;
use crate::ui::{ReportUnlessGone as _, hint, keys, method_badge};

type Command = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

const SHOWN: usize = 12;

#[derive(Clone)]
enum Target {
    Request { project: usize, path: PathBuf },
    Command(Command),
}

#[derive(Clone)]
struct Entry {
    label: String,
    detail: String,
    method: Option<String>,
    target: Target,
}

pub struct CommandPalette {
    workspace: WeakEntity<Workspace>,
    query: Entity<InputState>,
    entries: Vec<Entry>,
    selected: usize,
    _subscription: Subscription,
}

pub fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if window.has_active_dialog(cx) {
        return;
    }
    let entries = workspace.palette_entries(cx);
    let weak = cx.entity().downgrade();
    let palette = cx.new(|cx| CommandPalette::new(weak, entries, window, cx));
    window.open_dialog(cx, move |dialog, _, _| dialog.w(dp(640.)).margin_top(dp(90.)).close_button(false).child(palette.clone()));
}

impl Workspace {
    fn palette_entries(&self, cx: &App) -> Vec<Entry> {
        let mut entries = commands();
        for (index, view) in self.views.iter().enumerate() {
            let project = view.read(cx).project.read(cx);
            let project_name = project.collection.name();
            for request in project.collection.requests() {
                let relative = request.doc.path.strip_prefix(project.root()).unwrap_or(&request.doc.path).display().to_string();
                entries.push(Entry {
                    label: request.name(),
                    detail: format!("{project_name} · {relative}"),
                    method: Some(request.method()),
                    target: Target::Request { project: index, path: request.doc.path.clone() },
                });
            }
        }
        entries
    }

    fn open_from_palette(&mut self, project: usize, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.activate(project, window, cx);
        self.with_view(cx, |view, cx| view.open_request(&path, window, cx));
    }
}

fn commands() -> Vec<Entry> {
    let command = |label: &str, detail: &str, run: Command| Entry {
        label: label.into(),
        detail: detail.into(),
        method: None,
        target: Target::Command(run),
    };
    vec![
        command("New request", "Ctrl N", |workspace, window, cx| workspace.with_view(cx, |view, cx| view.new_request(None, window, cx))),
        command("New folder", "", |workspace, window, cx| workspace.with_view(cx, |view, cx| view.new_folder(None, window, cx))),
        command("Sync from OpenAPI", "", |workspace, window, cx| workspace.with_view(cx, |view, cx| view.sync_openapi(window, cx))),
        command("Environments…", "", |workspace, window, cx| workspace.with_view(cx, |view, cx| view.open_environments(window, cx))),
        command("Open collection…", "Ctrl O", Workspace::prompt_open),
        command("New collection…", "", Workspace::prompt_new_collection),
        command("Import collection…", "", Workspace::prompt_import),
        command("Export collection…", "", Workspace::prompt_export),
        command("Settings", "Ctrl ,", |_, window, cx| crate::settings::open(window, cx)),
        command("Switch light and dark theme", "", |_, _, cx| {
            let next = if gpui_component::Theme::global(cx).is_dark() { ThemeChoice::Light } else { ThemeChoice::Dark };
            appearance::set_theme(next, cx)
        }),
        command("Reload collection from disk", "Ctrl R", |workspace, _, cx| {
            workspace.with_view(cx, |view, cx| view.project.update(cx, |project, cx| project.reload(cx)))
        }),
        command("Close project", "Ctrl Shift W", |workspace, window, cx| workspace.close_project(workspace.active, window, cx)),
    ]
}

impl CommandPalette {
    fn new(workspace: WeakEntity<Workspace>, entries: Vec<Entry>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find a request or a command"));
        query.update(cx, |input, cx| input.focus(window, cx));
        let subscription = cx.subscribe_in(&query, window, |palette, _, event: &InputEvent, window, cx| match event {
            InputEvent::Change => {
                palette.selected = 0;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => palette.run(palette.selected, window, cx),
            _ => {}
        });
        Self { workspace, query, entries, selected: 0, _subscription: subscription }
    }

    fn matches(&self, cx: &App) -> Vec<Entry> {
        let query = self.query.read(cx).value().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut found: Vec<(usize, &Entry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let haystack = format!("{} {} {}", entry.label, entry.detail, entry.method.as_deref().unwrap_or_default()).to_lowercase();
                words
                    .iter()
                    .all(|word| haystack.contains(word))
                    .then(|| (if entry.label.to_lowercase().starts_with(&query) { 0 } else { 1 }, entry))
            })
            .collect();
        found.sort_by_key(|(rank, _)| *rank);
        found.into_iter().take(SHOWN).map(|(_, entry)| entry.clone()).collect()
    }

    fn run(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.matches(cx).into_iter().nth(index) else { return };
        window.close_dialog(cx);
        let target = entry.target;
        self.workspace
            .update(cx, |workspace, cx| match target {
                Target::Request { project, path } => workspace.open_from_palette(project, path, window, cx),
                Target::Command(run) => run(workspace, window, cx),
            })
            .report_unless_gone("run a palette entry");
    }

    fn move_selection(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let count = self.matches(cx).len().max(1);
        match event.keystroke.key.as_str() {
            "down" => self.selected = (self.selected + 1) % count,
            "up" => self.selected = (self.selected + count - 1) % count,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for CommandPalette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let selected_background = cx.theme().list_active;
        let matches = self.matches(cx);
        let selected = self.selected.min(matches.len().saturating_sub(1));
        v_flex()
            .gap(dp(10.))
            .on_key_down(cx.listener(|palette, event: &KeyDownEvent, _, cx| palette.move_selection(event, cx)))
            .child(Input::new(&self.query).large().prefix(Icon::new(IconName::Search).small()))
            .when(matches.is_empty(), |el| el.child(div().p_3().child(hint("Nothing matches.", cx))))
            .child(v_flex().gap(dp(2.)).children(matches.into_iter().enumerate().map(|(index, entry)| {
                let is_selected = index == selected;
                let is_command = entry.method.is_none();
                h_flex()
                    .id(("palette-entry", index))
                    .h(rems(2.4))
                    .px(dp(10.))
                    .gap(dp(10.))
                    .items_center()
                    .rounded(dp(9.))
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(selected_background))
                    .when(!is_selected, |el| el.hover(|el| el.bg(palette.hover)))
                    .child(match &entry.method {
                        Some(method) => method_badge(method, cx).into_any_element(),
                        None => div().w(rems(2.2)).child(Icon::new(IconName::Command).small().text_color(palette.faint)).into_any_element(),
                    })
                    .child(div().flex_none().text_color(palette.text).child(entry.label))
                    .child(div().flex_1())
                    .when(is_command && !entry.detail.is_empty(), |el| el.child(keys(&entry.detail, cx)))
                    .when(!is_command, |el| {
                        el.child(div().min_w_0().truncate().text_size(rems(0.76)).text_color(palette.faint).child(entry.detail))
                    })
                    .on_click(cx.listener(move |palette, _, window, cx| palette.run(index, window, cx)))
            })))
            .child(self.render_footer(cx))
    }
}

impl CommandPalette {
    fn render_footer(&self, cx: &App) -> impl IntoElement + use<> {
        let palette = appearance::palette(cx);
        let hint_pair =
            |combination: &str, action: &str| h_flex().gap(dp(6.)).items_center().child(keys(combination, cx)).child(action.to_string());
        h_flex()
            .pt(dp(10.))
            .gap(dp(16.))
            .border_t_1()
            .border_color(palette.border_soft)
            .text_size(rems(0.74))
            .text_color(palette.faint)
            .child(hint_pair("↑ ↓", "move"))
            .child(hint_pair("Enter", "open"))
            .child(hint_pair("Esc", "close"))
    }
}
