use std::path::Path;
use std::time::Duration;

use gpui::{Context, DragMoveEvent, Window};
use kestrel_core::collection_move::MoveTarget;

use super::ProjectView;
use super::dragging::TreeDrag;
use crate::ui::ReportUnlessGone as _;

impl ProjectView {
    pub(super) fn hover_tree_folder(&mut self, path: &Path, event: &DragMoveEvent<TreeDrag>, window: &mut Window, cx: &mut Context<Self>) {
        let drag = event.drag(cx);
        let project = self.project.read(cx);
        let can_expand = event.bounds.contains(&event.event.position)
            && !self.sidebar.filtered
            && !self.expanded.contains(path)
            && !project.tree_move_pending
            && project.collection.root == drag.root
            && project.collection.can_move_item(&drag.path, &MoveTarget::Inside(path.to_path_buf()));
        if !can_expand {
            if self.tree_hover.as_ref().is_some_and(|(hovered, _)| hovered == path) {
                self.tree_hover = None;
            }
            return;
        }
        if self.tree_hover.as_ref().is_some_and(|(hovered, _)| hovered == path) {
            return;
        }
        let hovered = path.to_path_buf();
        let bounds = event.bounds;
        self.tree_hover = Some((
            hovered.clone(),
            cx.spawn_in(window, async move |view, cx| {
                cx.background_executor().timer(Duration::from_millis(600)).await;
                view.update_in(cx, |view, window, cx| {
                    if cx.has_active_drag()
                        && !view.sidebar.filtered
                        && bounds.contains(&window.mouse_position())
                        && view.tree_hover.as_ref().is_some_and(|(path, _)| path == &hovered)
                    {
                        view.expanded.insert(hovered);
                        view.sidebar.show_expanded(&view.expanded);
                        cx.notify();
                    }
                })
                .report_unless_gone("expand a folder while dragging a collection item");
            }),
        ));
    }
}
