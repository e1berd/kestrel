use gpui::{Context, EntityId, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Window, div, relative, rems};
use gpui_component::h_flex;

use super::ProjectView;
use crate::appearance::{self, dp};
use crate::ui::method_label;

#[derive(Clone)]
pub(super) struct TabDrag {
    pub project: EntityId,
    pub tab: EntityId,
    pub title: String,
    pub method: String,
}

impl Render for TabDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        h_flex()
            .gap(dp(6.))
            .px(dp(12.))
            .py(dp(8.))
            .rounded(dp(9.))
            .border_1()
            .border_color(palette.accent)
            .bg(palette.raised)
            .text_color(palette.text)
            .text_size(rems(0.84))
            .child(method_label(&self.method, cx))
            .child(self.title.clone())
    }
}

impl ProjectView {
    pub(super) fn render_tab_drop_zones(&self, target: EntityId, cx: &mut Context<Self>) -> impl IntoElement {
        let project = cx.entity_id();
        let palette = appearance::palette(cx);
        h_flex().absolute().inset_0().children([false, true].map(|after| {
            div()
                .id(if after { "after-tab" } else { "before-tab" })
                .w(relative(0.5))
                .h_full()
                .can_drop(move |value, _, _| {
                    value.downcast_ref::<TabDrag>().is_some_and(|drag| drag.project == project && drag.tab != target)
                })
                .drag_over::<TabDrag>(move |style, _, _, _| {
                    if after { style.border_r_2().border_color(palette.accent) } else { style.border_l_2().border_color(palette.accent) }
                })
                .on_drop(cx.listener(move |view, drag: &TabDrag, _, cx| {
                    if drag.project != cx.entity_id() {
                        return;
                    }
                    let Some(source) = view.tabs.iter().position(|tab| tab.entity_id() == drag.tab) else { return };
                    let Some(target) = view.tabs.iter().position(|tab| tab.entity_id() == target) else { return };
                    view.active = reorder_tab(&mut view.tabs, source, target, after, view.active);
                    cx.notify();
                }))
        }))
    }
}

fn reorder_tab<T>(tabs: &mut Vec<T>, source: usize, target: usize, after: bool, active: usize) -> usize {
    let insertion = target + usize::from(after);
    let destination = insertion - usize::from(source < insertion);
    let tab = tabs.remove(source);
    tabs.insert(destination, tab);
    if active == source {
        destination
    } else {
        let active_after_removal = active - usize::from(active > source);
        active_after_removal + usize::from(active_after_removal >= destination)
    }
}

#[cfg(test)]
mod tests {
    use super::reorder_tab;

    #[test]
    fn dropping_before_or_after_any_tab_preserves_the_active_request() {
        for source in 0..4 {
            for target in 0..4 {
                for after in [false, true] {
                    for active in 0..4 {
                        let mut tabs = vec![0, 1, 2, 3];
                        let selected = tabs[active];
                        let active = reorder_tab(&mut tabs, source, target, after, active);
                        assert_eq!(tabs[active], selected);
                        let source_position = tabs.iter().position(|tab| *tab == source).unwrap();
                        let target_position = tabs.iter().position(|tab| *tab == target).unwrap();
                        if source != target {
                            assert_eq!(source_position + usize::from(!after), target_position + usize::from(after));
                        }
                    }
                }
            }
        }
    }
}
