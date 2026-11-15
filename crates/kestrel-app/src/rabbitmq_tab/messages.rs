use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, Window, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Editor;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;
use kestrel_core::rabbitmq::{Message, MessageState, Settlement};

use super::RabbitMqTab;
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{chip, code_box, format_size, hint, icon_tile, name_value_rows, scroll, section_tab, section_tab_strip};

impl RabbitMqTab {
    pub(super) fn append_message(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        if self.messages.len() >= 200
            && let Some(index) = self.messages.iter().position(|message| message.state != MessageState::Pending)
        {
            self.messages.remove(index);
            self.selected =
                self.selected.and_then(|selected| if selected == index { None } else { Some(selected - usize::from(selected > index)) });
        }
        let select = self.selected.is_none();
        self.messages.push(message);
        if select {
            self.select_message(self.messages.len() - 1, window, cx);
        }
    }

    fn select_message(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(message) = self.messages.get(index) else { return };
        let text = String::from_utf8_lossy(&message.body).into_owned();
        let mut text = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(json) => match serde_json::to_string_pretty(&json) {
                Ok(text) => text,
                Err(error) => {
                    kestrel_core::report::report_error("format a RabbitMQ message", error);
                    text
                }
            },
            Err(_) => text,
        };
        if message.body_bytes > message.body.len() {
            text.push_str("\n\n… payload preview truncated to 256 KB");
        }
        self.message_body.update(cx, |editor, cx| editor.set_value(text, window, cx));
        self.selected = Some(index);
        cx.notify();
    }

    pub(super) fn render_messages(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let pending = self.messages.iter().filter(|message| message.state == MessageState::Pending).count();
        let header = h_flex()
            .flex_wrap()
            .px(dp(16.))
            .py(dp(12.))
            .gap(dp(8.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Messages"))
            .child(chip(self.messages.len().to_string(), palette.muted, cx))
            .when(pending > 0, |element| element.child(chip(format!("{pending} pending"), palette.warning, cx)))
            .child(div().flex_1())
            .child(Button::new("rmq-clear").ghost().small().label("Clear settled").on_click(cx.listener(|tab, _, _, cx| {
                tab.messages.retain(|message| message.state == MessageState::Pending);
                tab.selected = None;
                cx.notify();
            })));
        let mut panel = v_flex().size_full().child(header);
        if self.messages.is_empty() {
            return panel
                .child(
                    v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap(dp(10.))
                        .p(dp(24.))
                        .child(icon_tile(IconName::Send, 44., cx))
                        .child(div().font_weight(FontWeight::MEDIUM).child("No messages yet"))
                        .child(hint("Connect to a broker, then publish or receive a message.", cx)),
                )
                .into_any_element();
        }
        let rows = v_flex().children(self.messages.iter().enumerate().rev().map(|(index, message)| {
            let selected = self.selected == Some(index);
            let destination = if message.routing_key.is_empty() { "(no routing key)" } else { &message.routing_key };
            h_flex()
                .id(("rmq-message", index))
                .px(dp(12.))
                .py(dp(8.))
                .gap(dp(8.))
                .rounded(dp(8.))
                .cursor_pointer()
                .text_size(rems(0.8))
                .when(selected, |element| element.bg(palette.selection))
                .when(!selected, |element| element.hover(|element| element.bg(palette.hover)))
                .child(
                    div()
                        .text_color(if message.delivery_tag.is_none() { palette.accent } else { palette.success })
                        .child(if message.delivery_tag.is_none() { "Sent" } else { "Received" }),
                )
                .child(div().flex_1().min_w_0().truncate().font_family(MONO_FONT).child(destination.to_string()))
                .child(hint(message.state.label(), cx))
                .on_click(cx.listener(move |tab, _, window, cx| tab.select_message(index, window, cx)))
        }));
        panel = panel.child(div().h(rems(10.)).min_h(rems(4.)).px(dp(8.)).child(scroll("rmq-message-list", rows.into_any_element())));
        if let Some(message) = self.selected.and_then(|index| self.messages.get(index)) {
            panel = panel.child(self.render_message_details(message, cx));
        } else {
            panel = panel.child(div().p(dp(16.)).child(hint("Select a message to inspect its payload and properties.", cx)));
        }
        panel.into_any_element()
    }

    fn render_message_details(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let pending = message.state == MessageState::Pending && self.connected && !self.busy;
        let mut details = v_flex().flex_1().min_h_0().gap(dp(10.)).p(dp(16.));
        details = details.child(
            h_flex()
                .flex_wrap()
                .gap(dp(8.))
                .child(chip(message.state.label(), if pending { palette.warning } else { palette.muted }, cx))
                .child(hint(&format_size(message.body_bytes), cx))
                .when(message.redelivered, |element| element.child(chip("Redelivered", palette.warning, cx))),
        );
        if message.delivery_tag.is_some() {
            details = details.child(
                h_flex().flex_wrap().gap(dp(6.)).children(
                    [
                        ("rmq-ack", "Ack", Settlement::Ack),
                        ("rmq-requeue", "Requeue", Settlement::Requeue),
                        ("rmq-reject", "Reject", Settlement::Reject),
                    ]
                    .into_iter()
                    .map(|(id, label, settlement)| {
                        Button::new(id)
                            .small()
                            .label(label)
                            .disabled(!pending)
                            .on_click(cx.listener(move |tab, _, window, cx| tab.settle(settlement, window, cx)))
                    }),
                ),
            );
        }
        let tabs = [(false, "Payload"), (true, "Properties")]
            .into_iter()
            .enumerate()
            .map(|(index, (properties, label))| {
                section_tab(("rmq-message-tab", index), label, None, self.show_properties == properties, cx).on_click(cx.listener(
                    move |tab, _, _, cx| {
                        tab.show_properties = properties;
                        cx.notify();
                    },
                ))
            })
            .collect::<Vec<_>>();
        let content = if self.show_properties {
            let mut rows = vec![
                ("Exchange".into(), if message.exchange.is_empty() { "(default)".into() } else { message.exchange.clone() }),
                ("Routing key".into(), message.routing_key.clone()),
            ];
            rows.extend(message.properties.clone());
            scroll("rmq-properties", name_value_rows(rows, cx).into_any_element())
        } else {
            code_box(Editor::new(&self.message_body).bordered(false).readonly(true).h_full(), cx).into_any_element()
        };
        details.child(section_tab_strip("rmq-message-tabs", tabs, cx)).child(div().flex_1().min_h_0().child(content)).into_any_element()
    }
}
