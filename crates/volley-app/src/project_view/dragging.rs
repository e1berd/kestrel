use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
    relative, rems,
};
use gpui_component::{Icon, h_flex, v_flex};
use gpui_kit_assets::IconName;
use volley_core::Collection;
use volley_core::collection_move::MoveTarget;
use volley_core::report::report_error;

use super::ProjectView;
use crate::appearance::{self, dp};
use crate::ui::{ReportUnlessGone as _, eyebrow, method_badge};

#[derive(Clone)]
pub(super) struct TreeDrag {
    pub root: PathBuf,
    pub path: PathBuf,
    pub name: String,
    pub method: Option<String>,
}

impl Render for TreeDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        h_flex()
            .gap(dp(8.))
            .px(dp(10.))
            .py(dp(6.))
            .rounded(dp(8.))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .text_color(palette.text)
            .text_size(rems(0.86))
            .child(match &self.method {
                Some(method) => method_badge(method, cx).into_any_element(),
                None => Icon::new(IconName::Folder).size(dp(14.)).into_any_element(),
            })
            .child(self.name.clone())
    }
}

impl ProjectView {
    pub(super) fn render_collection_drop_label(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_drop_zone(MoveTarget::Inside(self.root(cx)), cx)
            .id("collection-root-drop")
            .rounded(dp(6.))
            .child(eyebrow("Requests", cx))
    }

    pub(super) fn render_drop_zones(&self, path: &Path, is_folder: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let path = path.to_path_buf();
        v_flex()
            .absolute()
            .inset_0()
            .child(self.render_drop_zone(MoveTarget::Before(path.clone()), cx).h(relative(if is_folder { 0.25 } else { 0.5 })))
            .when(is_folder, |element| element.child(self.render_drop_zone(MoveTarget::Inside(path.clone()), cx).h(relative(0.5))))
            .child(self.render_drop_zone(MoveTarget::After(path), cx).h(relative(if is_folder { 0.25 } else { 0.5 })))
    }

    pub(super) fn render_root_drop_zone(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        self.render_drop_zone(MoveTarget::Inside(self.root(cx)), cx)
            .flex_1()
            .min_h(rems(2.))
            .rounded(dp(8.))
            .when(cx.has_active_drag(), |element| {
                element.flex().items_center().justify_center().text_xs().text_color(palette.muted).child("Move to collection root")
            })
    }

    fn render_drop_zone(&self, target: MoveTarget, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let view = cx.entity().downgrade();
        let drop_target = target.clone();
        let palette = appearance::palette(cx);
        let before = matches!(target, MoveTarget::Before(_));
        let after = matches!(target, MoveTarget::After(_));
        div()
            .id(ElementId::Name(format!("drop-{target:?}").into()))
            .w_full()
            .can_drop(move |value, _, cx| {
                let Some(drag) = value.downcast_ref::<TreeDrag>() else { return false };
                let Some(view) = view.upgrade() else { return false };
                let project = view.read(cx).project.read(cx);
                !project.tree_move_pending && project.collection.root == drag.root && project.collection.can_move_item(&drag.path, &target)
            })
            .drag_over::<TreeDrag>(move |style, _, _, _| {
                if before {
                    style.border_t_2().border_color(palette.accent)
                } else if after {
                    style.border_b_2().border_color(palette.accent)
                } else {
                    style.bg(palette.selection)
                }
            })
            .on_drop(cx.listener(move |view, drag: &TreeDrag, window, cx| {
                view.move_tree_item(drag.path.clone(), drop_target.clone(), window, cx);
            }))
    }

    fn move_tree_item(&mut self, path: PathBuf, target: MoveTarget, window: &mut Window, cx: &mut Context<Self>) {
        if self.project.read(cx).tree_move_pending {
            return;
        }
        self.project.update(cx, |project, _| project.tree_move_pending = true);
        let root = self.root(cx);
        let task = cx.background_spawn(async move {
            let mut collection = Collection::open(&root)?;
            let new_path = collection.move_item(&path, &target)?;
            anyhow::Ok((collection, path, new_path))
        });
        let project = self.project.clone();
        cx.spawn_in(window, async move |view, cx| {
            match task.await {
                Ok((collection, old, new)) => {
                    project.update(cx, |project, _| {
                        project.collection = collection;
                        project.load_error = None;
                    });
                    view.update_in(cx, |view, window, cx| view.relocate_tree_state(&old, &new, window, cx))
                        .report_unless_gone("update tabs after moving a collection item");
                }
                Err(error) => {
                    report_error("move a collection item", &error);
                    view.update_in(cx, |_, window, cx| Self::notify_error(window, cx, error))
                        .report_unless_gone("show a collection move error");
                }
            }
            project.update(cx, |project, cx| project.finish_tree_move(cx));
        })
        .detach();
        cx.notify();
    }

    fn relocate_tree_state(&mut self, old: &Path, new: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.expanded = self
            .expanded
            .iter()
            .map(|path| path.strip_prefix(old).map(|relative| new.join(relative)).unwrap_or_else(|_| path.clone()))
            .collect();
        self.expand_ancestors(new, cx);
        if let Some(parent) = new.parent() {
            self.expanded.insert(parent.to_path_buf());
        }
        for tab in &self.tabs {
            tab.update(cx, |tab, cx| tab.relocate(old, new, window, cx));
        }
        cx.notify();
    }
}
