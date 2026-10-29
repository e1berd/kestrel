use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, HighlightStyle, Hsla, InteractiveElement as _, IntoElement, MouseButton, MouseUpEvent, ParentElement as _,
    Styled as _, Window, div, point,
};
use gpui_component::input::{EditorState, InlineToken, InlineTokenClickEvent, Input, TextDecoration};
use volley_core::Plan;
use volley_core::report::ReportExt as _;
use volley_core::vars::Scope;

use super::RequestTab;
use crate::appearance::{self, MONO_FONT, dp, palette::Palette};
use crate::ui::ReportUnlessGone as _;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum VariableState {
    Set,
    Empty,
    Missing,
}

impl VariableState {
    fn of(name: &str, scope: Option<&Scope>) -> Self {
        match scope.and_then(|scope| scope.get(name)) {
            Some(value) if value.is_empty() => VariableState::Empty,
            Some(_) => VariableState::Set,
            None => VariableState::Missing,
        }
    }

    pub(super) fn color(self, palette: &Palette) -> Hsla {
        match self {
            VariableState::Set => palette.success,
            VariableState::Empty | VariableState::Missing => palette.danger,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            VariableState::Set => "Set",
            VariableState::Empty => "Empty",
            VariableState::Missing => "Not set",
        }
    }
}

impl RequestTab {
    pub(super) fn tokenize_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.url.read(cx).value().to_string();
        let taken: Vec<Range<usize>> = self.url.read(cx).tokens().iter().map(|span| span.range()).collect();
        let fresh: Vec<(Range<usize>, String)> = placeholders(&text)
            .into_iter()
            .filter(|(range, _)| !taken.iter().any(|token| token.start < range.end && range.start < token.end))
            .collect();
        for (range, name) in fresh.into_iter().rev() {
            let token = InlineToken::new(name.clone(), text[range.clone()].to_string()).with_label(name);
            self.url
                .update(cx, |input, cx| input.replace_range_with_token(range, token, window, cx))
                .report_err("turn a variable into a token");
        }
    }

    pub(super) fn render_url_input(&self, cx: &mut Context<Self>) -> Input {
        let scope = self.current_scope(cx);
        let states: HashMap<String, VariableState> = placeholders(&self.url.read(cx).value())
            .into_iter()
            .map(|(_, name)| {
                let state = VariableState::of(&name, scope.as_ref());
                (name, state)
            })
            .collect();
        let states = Rc::new(states);
        let palette = appearance::palette(cx);
        let tab = cx.entity().downgrade();
        Input::new(&self.url)
            .appearance(false)
            .token(move |token, _, _| {
                let name = token.token().id().to_string();
                let color = states.get(&name).copied().unwrap_or(VariableState::Missing).color(&palette);
                div()
                    .h(token.line_height())
                    .px(dp(5.))
                    .flex()
                    .items_center()
                    .rounded(dp(5.))
                    .bg(palette.tint(color))
                    .text_color(color)
                    .font_family(MONO_FONT)
                    .when(token.is_selected(), |el| el.bg(palette.selection))
                    .child(format!("{{{{{name}}}}}"))
            })
            .on_token_click(move |event: &InlineTokenClickEvent, window, cx| {
                let name = event.token().id().to_string();
                let anchor = point(event.bounds().left(), event.bounds().bottom() + dp(6.));
                tab.update(cx, |tab, cx| tab.open_variable_popup(name, anchor, window, cx)).report_unless_gone("open a variable");
            })
    }

    pub(super) fn refresh_variable_marks(&mut self, cx: &mut Context<Self>) {
        let scope = self.current_scope(cx);
        let palette = appearance::palette(cx);
        let editors = [self.body.clone(), self.graphql_vars.clone()];
        self.editor_marks.retain(|(id, _)| editors.iter().any(|editor| editor.entity_id() == *id));
        for editor in editors {
            let marks: Vec<TextDecoration> = placeholders(&editor.read(cx).value())
                .into_iter()
                .map(|(range, name)| {
                    let color = VariableState::of(&name, scope.as_ref()).color(&palette);
                    let style = HighlightStyle { color: Some(color), background_color: Some(palette.tint(color)), ..Default::default() };
                    TextDecoration::new(range, style)
                })
                .collect();
            match self.editor_marks.iter().find(|(id, _)| *id == editor.entity_id()) {
                Some((_, collection)) => collection.set(marks, cx),
                None => {
                    let collection = editor.update(cx, |state, cx| state.create_decorations_collection(marks, cx));
                    self.editor_marks.push((editor.entity_id(), collection));
                }
            }
        }
    }

    pub(super) fn with_clickable_variables(
        &self,
        editor: &Entity<EditorState>,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = editor.clone();
        div()
            .size_full()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |tab, event: &MouseUpEvent, window, cx| {
                    let state = editor.read(cx);
                    if !state.selected_range().is_empty() {
                        return;
                    }
                    let cursor = state.cursor();
                    let hit = placeholders(&state.value()).into_iter().find(|(range, _)| range.start < cursor && cursor < range.end);
                    if let Some((_, name)) = hit {
                        tab.open_variable_popup(name, point(event.position.x, event.position.y + dp(14.)), window, cx);
                    }
                }),
            )
            .child(content)
            .into_any_element()
    }

    fn current_scope(&self, cx: &mut Context<Self>) -> Option<Scope> {
        let file = self.build_file(cx);
        let project = self.project.read(cx);
        Plan::with_file(&project.collection, &self.path, file, project.env.as_deref()).ok().map(|plan| plan.scope(&project.session))
    }
}

fn placeholders(text: &str) -> Vec<(Range<usize>, String)> {
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(start) = text[offset..].find("{{").map(|index| offset + index) {
        let Some(end) = text[start + 2..].find("}}").map(|index| start + 2 + index) else { break };
        let name = text[start + 2..end].trim();
        let is_generated = name.starts_with('$');
        if !name.is_empty() && !is_generated && !name.contains("{{") {
            found.push((start..end + 2, name.to_string()));
        }
        offset = end + 2;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::placeholders;

    #[test]
    fn placeholders_skip_generated_values() {
        let found = placeholders("{{baseUrl}}/users/{{ id }}?r={{$randomUUID}}&q={{");
        assert_eq!(found, vec![(0..11, "baseUrl".to_string()), (18..26, "id".to_string())]);
    }
}
