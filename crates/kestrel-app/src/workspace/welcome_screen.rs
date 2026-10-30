use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, Window, div, img, rems,
};
use gpui_component::{Icon, IconName, h_flex, v_flex};

use super::Workspace;
use crate::appearance::{self, dp, elevation};
use crate::assets;
use crate::prefs::Prefs;
use crate::ui::{eyebrow, icon_tile};

type WelcomeAction = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

const ACTIONS: [(&str, &str, &str, IconName, WelcomeAction); 3] = [
    ("welcome-open", "Open collection", "A Bruno 3 or Kestrel folder", IconName::FolderOpen, Workspace::prompt_open),
    ("welcome-new", "New collection", "Start empty in any folder", IconName::Plus, Workspace::prompt_new_collection),
    ("welcome-import", "Import collection", "OpenAPI, Swagger or Apidog", IconName::FileText, Workspace::prompt_import),
];

impl Workspace {
    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let recent: Vec<PathBuf> =
            Prefs::load().recent.into_iter().filter(|path| path.join(kestrel_core::collection::COLLECTION_FILE).is_file()).collect();
        let palette = appearance::palette(cx);
        let actions: Vec<_> = ACTIONS.into_iter().map(|action| self.render_action_card(action, cx)).collect();
        div()
            .size_full()
            .pl(dp(4.))
            .pb(dp(2.))
            .child(
                v_flex()
                    .id("welcome")
                    .size_full()
                    .overflow_y_scroll()
                    .items_center()
                    .justify_center()
                    .gap(dp(28.))
                    .py(dp(32.))
                    .rounded(dp(14.))
                    .border_1()
                    .border_color(palette.border_soft)
                    .bg(palette.background)
                    .shadow(elevation::card(cx))
                    .child(self.render_welcome_hero(cx))
                    .child(h_flex().gap(dp(12.)).children(actions))
                    .when(!recent.is_empty(), |el| el.child(self.render_recent(recent, cx))),
            )
            .into_any_element()
    }

    fn render_welcome_hero(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        v_flex()
            .items_center()
            .gap(dp(6.))
            .child(img(assets::LOGO).size(dp(88.)).mb(dp(10.)))
            .child(div().text_size(rems(1.9)).font_weight(FontWeight::BOLD).child("Kestrel"))
            .child(div().text_color(palette.muted).child("A free API client that keeps your requests as files in git."))
    }

    fn render_action_card(
        &self,
        (id, title, description, icon, action): (&'static str, &str, &str, IconName, WelcomeAction),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = appearance::palette(cx);
        v_flex()
            .id(id)
            .w(dp(200.))
            .p(dp(16.))
            .gap(dp(4.))
            .rounded(dp(12.))
            .border_1()
            .border_color(palette.border_soft)
            .bg(palette.raised)
            .cursor_pointer()
            .hover(|el| el.border_color(palette.border).bg(palette.hover))
            .child(div().pb(dp(8.)).child(icon_tile(icon, 36., cx)))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
            .child(div().text_size(rems(0.8)).text_color(palette.muted).child(description.to_string()))
            .on_click(cx.listener(move |workspace, _: &ClickEvent, window, cx| action(workspace, window, cx)))
            .into_any_element()
    }

    fn render_recent(&self, recent: Vec<PathBuf>, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        v_flex().w(dp(624.)).gap(dp(6.)).child(div().px(dp(10.)).child(eyebrow("Recent", cx))).children(recent.into_iter().enumerate().map(
            |(index, path)| {
                let name = kestrel_core::collection::file_stem(&path);
                let location = path.parent().map(|parent| parent.display().to_string()).unwrap_or_default();
                h_flex()
                    .id(("recent", index))
                    .px(dp(10.))
                    .py(dp(8.))
                    .gap(dp(10.))
                    .items_center()
                    .rounded(dp(9.))
                    .cursor_pointer()
                    .hover(|el| el.bg(palette.hover))
                    .child(Icon::new(IconName::Folder).size(dp(15.)).text_color(palette.faint))
                    .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(name))
                    .child(div().flex_1().min_w_0().truncate().text_size(rems(0.8)).text_color(palette.faint).child(location))
                    .on_click(cx.listener(move |workspace, _, window, cx| workspace.open_collection(&path, window, cx)))
            },
        ))
    }
}
