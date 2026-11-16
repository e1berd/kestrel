use anyhow::Result;
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Sizable as _, h_flex, v_flex};
use serde_json::{Map, Value};

use super::{RequestTab, graphql_syntax};
use crate::appearance::{self, dp};
use crate::ui::{hint, section_tab, section_tab_strip};

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Section {
    #[default]
    Query,
    Variables,
}

impl RequestTab {
    pub(super) fn render_graphql(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let tabs = [(Section::Query, "Query"), (Section::Variables, "Variables")]
            .into_iter()
            .enumerate()
            .map(|(index, (section, label))| {
                section_tab(("graphql-section", index), label, None, section == self.graphql_section, cx).on_click(cx.listener(
                    move |tab, _, _, cx| {
                        tab.graphql_section = section;
                        cx.notify();
                    },
                ))
            })
            .collect::<Vec<_>>();
        let editor = if self.graphql_section == Section::Query { &self.body } else { &self.graphql_vars };
        let content = self.render_code_with_variables(editor, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .gap(dp(12.))
            .child(section_tab_strip("graphql-sections", tabs, cx))
            .when(self.graphql_section == Section::Variables, |element| {
                element.child(
                    h_flex()
                        .flex_wrap()
                        .gap(dp(8.))
                        .items_center()
                        .child(
                            Button::new("graphql-add-variables")
                                .small()
                                .ghost()
                                .label("Add from query")
                                .tooltip("Add missing variable names from the query; existing values are preserved")
                                .on_click(cx.listener(|tab, _, window, cx| tab.edit_graphql_variables(true, window, cx))),
                        )
                        .child(
                            Button::new("graphql-format-variables")
                                .small()
                                .ghost()
                                .label("Format JSON")
                                .on_click(cx.listener(|tab, _, window, cx| tab.edit_graphql_variables(false, window, cx))),
                        ),
                )
            })
            .when_some(self.graphql_error.clone(), |element, error| element.child(div().text_sm().text_color(palette.danger).child(error)))
            .child(div().flex_1().min_h_0().child(content))
            .when(self.graphql_section == Section::Variables, |element| {
                element.child(hint(
                    "JSON object, for example {\"id\": \"123\"}. Added variables start as null; replace them with your values.",
                    cx,
                ))
            })
            .into_any_element()
    }

    fn edit_graphql_variables(&mut self, add_from_query: bool, window: &mut Window, cx: &mut Context<Self>) {
        let query = if add_from_query { self.body.read(cx).value().to_string() } else { String::new() };
        let original = self.graphql_vars.read(cx).value().to_string();
        match prepare_variables(&original, &query) {
            Ok(text) => {
                self.graphql_error = None;
                self.graphql_vars.update(cx, |editor, cx| editor.set_value(text, window, cx));
                self.changed(cx);
            }
            Err(error) => {
                kestrel_core::report::report_error("edit GraphQL variables", &error);
                self.graphql_error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }
}

fn prepare_variables(original: &str, query: &str) -> Result<String> {
    let value = kestrel_core::graphql::parse_variables(original)?;
    let mut variables = match value {
        Value::Object(variables) => variables,
        _ => Map::new(),
    };
    for name in graphql_syntax::variable_names(query) {
        variables.entry(name).or_insert(Value::Null);
    }
    Ok(serde_json::to_string_pretty(&Value::Object(variables))?)
}

#[cfg(test)]
mod tests {
    use super::prepare_variables;

    #[test]
    fn adding_query_variables_keeps_existing_nested_values_and_ignores_comments_and_strings() {
        let query = "query($id: ID!, $filter: Filter) { user(id: $id) { name(text: \"$ignored\") } } # $comment";
        let original = r#"{"id":"{{userId}}","filter":{"tags":["a","b"]},"extra":42}"#;
        let text = prepare_variables(original, query).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap(), serde_json::from_str::<serde_json::Value>(original).unwrap());
        let added: serde_json::Value = serde_json::from_str(&prepare_variables("", query).unwrap()).unwrap();
        assert_eq!(added, serde_json::json!({"id": null, "filter": null}));
    }

    #[test]
    fn invalid_json_and_non_object_variables_are_rejected_without_replacing_the_input() {
        for original in ["{", "[1]", "123", "true", "\"text\""] {
            assert!(prepare_variables(original, "").is_err());
        }
        assert_eq!(prepare_variables("", "").unwrap(), "{}");
    }
}
