//! A table of name–value rows with a switch each: parameters, headers, form fields, variables. There
//! is always an empty row at the end; typing into it adds the next.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use volley_core::format::KeyValue;

pub struct KvChanged;

struct Row {
    /// The row as it was read, so what the table does not show (description, type, …) is kept.
    base: KeyValue,
    enabled: bool,
    name: Entity<InputState>,
    value: Entity<InputState>,
    _subscriptions: [Subscription; 2],
}

pub struct KvTable {
    id: SharedString,
    rows: Vec<Row>,
    /// The `type` new rows get (`query` for parameters).
    new_kind: Option<String>,
    name_placeholder: SharedString,
    value_placeholder: SharedString,
    /// Whether a row's `type` shows as a tag (path or query).
    show_kind: bool,
}

impl EventEmitter<KvChanged> for KvTable {}

impl KvTable {
    pub fn new(id: &str, items: &[KeyValue], window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut table = Self {
            id: id.to_string().into(),
            rows: Vec::new(),
            new_kind: None,
            name_placeholder: "Name".into(),
            value_placeholder: "Value".into(),
            show_kind: false,
        };
        table.set_items(items, window, cx);
        table
    }

    pub fn new_kind(mut self, kind: &str) -> Self {
        self.new_kind = Some(kind.into());
        self.show_kind = true;
        self
    }

    pub fn placeholders(mut self, name: &str, value: &str) -> Self {
        self.name_placeholder = name.to_string().into();
        self.value_placeholder = value.to_string().into();
        self
    }

    pub fn set_items(&mut self, items: &[KeyValue], window: &mut Window, cx: &mut Context<Self>) {
        self.rows.clear();
        for item in items {
            self.push_row(item.clone(), window, cx);
        }
        self.ensure_blank(window, cx);
        cx.notify();
    }

    fn push_row(&mut self, base: KeyValue, window: &mut Window, cx: &mut Context<Self>) {
        let name_placeholder = self.name_placeholder.clone();
        let value_placeholder = self.value_placeholder.clone();
        let name_text = base.name.clone();
        let value_text = base.value.clone();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(name_placeholder).default_value(name_text));
        let value = cx.new(|cx| InputState::new(window, cx).placeholder(value_placeholder).default_value(value_text));
        let on_change = |this: &mut Self, _: &Entity<InputState>, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>| {
            if matches!(event, InputEvent::Change) {
                this.ensure_blank(window, cx);
                cx.emit(KvChanged);
            }
        };
        let subscriptions = [cx.subscribe_in(&name, window, on_change), cx.subscribe_in(&value, window, on_change)];
        self.rows.push(Row { enabled: !base.disabled, base, name, value, _subscriptions: subscriptions });
    }

    fn is_blank(row: &Row, cx: &gpui::App) -> bool {
        row.name.read(cx).value().is_empty() && row.value.read(cx).value().is_empty()
    }

    fn ensure_blank(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rows.last().is_none_or(|row| !Self::is_blank(row, cx)) {
            let base = KeyValue { kind: self.new_kind.clone(), ..Default::default() };
            self.push_row(base, window, cx);
            cx.notify();
        }
    }

    /// The rows as they would be saved, without the empty one at the end.
    pub fn items(&self, cx: &gpui::App) -> Vec<KeyValue> {
        self.rows
            .iter()
            .filter(|row| !Self::is_blank(row, cx))
            .map(|row| KeyValue {
                name: row.name.read(cx).value().to_string(),
                value: row.value.read(cx).value().to_string(),
                disabled: !row.enabled,
                ..row.base.clone()
            })
            .collect()
    }

    /// Adds a row unless one with that name and type is there.
    pub fn ensure(&mut self, item: KeyValue, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.rows.iter().any(|row| row.base.kind == item.kind && row.name.read(cx).value() == item.name.as_str()) {
            return false;
        }
        let blank = self.rows.pop();
        self.push_row(item, window, cx);
        if let Some(blank) = blank {
            if Self::is_blank(&blank, cx) { self.rows.push(blank) } else { self.rows.insert(self.rows.len() - 1, blank) }
        }
        self.ensure_blank(window, cx);
        cx.notify();
        true
    }

    /// Drops rows of `kind` whose names `keep` does not hold and that have no value.
    pub fn retain_kind(&mut self, kind: &str, keep: &[String], cx: &mut Context<Self>) -> bool {
        let before = self.rows.len();
        self.rows.retain(|row| {
            row.base.kind.as_deref() != Some(kind)
                || Self::is_blank(row, cx)
                || keep.iter().any(|k| k.as_str() == row.name.read(cx).value().as_str())
                || !row.value.read(cx).value().is_empty()
        });
        let changed = before != self.rows.len();
        if changed {
            cx.notify();
        }
        changed
    }

    fn toggle(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get_mut(index) {
            row.enabled = enabled;
            cx.emit(KvChanged);
            cx.notify();
        }
    }

    fn remove(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.rows.len() {
            self.rows.remove(index);
            self.ensure_blank(window, cx);
            cx.emit(KvChanged);
            cx.notify();
        }
    }
}

impl Render for KvTable {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let last = self.rows.len().saturating_sub(1);
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        v_flex().w_full().gap_1().children(self.rows.iter().enumerate().map(|(index, row)| {
            let blank = index == last;
            let kind = row.base.kind.clone().filter(|_| self.show_kind && !blank);
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(div().w(px(20.)).when(!blank, |el| {
                    el.child(
                        Checkbox::new((self.id.clone(), index))
                            .checked(row.enabled)
                            .on_click(cx.listener(move |table, checked: &bool, _, cx| table.toggle(index, *checked, cx))),
                    )
                }))
                .child(div().w(gpui::relative(0.35)).flex_none().min_w_0().child(Input::new(&row.name).small()))
                .child(div().flex_1().min_w_0().child(Input::new(&row.value).small()))
                .child(div().w(px(44.)).text_xs().text_color(muted).when_some(kind, |el, kind| el.child(kind)))
                .child(div().w(px(24.)).when(!blank, |el| {
                    el.child(
                        Button::new((SharedString::from(format!("{}-remove", self.id)), index))
                            .xsmall()
                            .ghost()
                            .icon(IconName::Close)
                            .on_click(cx.listener(move |table, _, window, cx| table.remove(index, window, cx))),
                    )
                }))
                .border_b_1()
                .border_color(border)
                .pb_1()
        }))
    }
}
