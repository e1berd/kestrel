use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Window, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Editor, Input};
use gpui_component::{Disableable as _, h_flex, h_resizable, resizable_panel, v_flex};

use super::{RabbitMqChanged, RabbitMqTab, Section};
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{ReportUnlessGone as _, chip, code_box, hint, labelled_input, scroll, section_tab, section_tab_strip, segmented};

impl RabbitMqTab {
    pub fn render_connection_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let active = self.client.is_some();
        h_flex()
            .flex_1()
            .min_w_0()
            .h(rems(2.8))
            .p(dp(4.))
            .gap(dp(8.))
            .rounded(dp(12.))
            .border_1()
            .border_color(palette.border)
            .bg(palette.raised)
            .shadow(appearance::elevation::lifted(cx))
            .child(div().px(dp(10.)).text_sm().text_color(palette.muted).child("Broker"))
            .child(div().flex_1().min_w_0().font_family(MONO_FONT).child(Input::new(self.input("url")).appearance(false).disabled(active)))
            .child(
                Button::new("rmq-connect")
                    .label(if active { "Disconnect" } else { "Connect" })
                    .when(!active, |button| button.primary())
                    .on_click(cx.listener(|tab, _, window, cx| tab.toggle_connection(window, cx))),
            )
            .into_any_element()
    }

    fn render_connection_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        h_flex()
            .px(dp(16.))
            .pb(dp(12.))
            .gap(dp(8.))
            .items_center()
            .child(chip(self.status.clone(), if self.connected { palette.success } else { palette.muted }, cx))
            .child(hint("AMQP 0-9-1 · Manual acknowledgements", cx))
    }

    fn render_operations(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (Section::Publish, "Publish"),
            (Section::Consume, "Receive"),
            (Section::Connection, "Connection"),
            (Section::Headers, "Headers"),
            (Section::Variables, "Vars"),
            (Section::Docs, "Docs"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (section, label))| {
            section_tab(("rmq-section", index), label, None, self.section == section, cx)
                .on_click(cx.listener(move |tab, _, _, cx| tab.select_section(section, cx)))
        })
        .collect::<Vec<_>>();
        let content = match self.section {
            Section::Publish => self.render_publish(cx),
            Section::Consume => self.render_consume(cx),
            Section::Connection => self.render_connection_settings(cx),
            Section::Headers => scroll("rmq-header-scroll", self.headers.clone().into_any_element()),
            Section::Variables => v_flex()
                .size_full()
                .gap(dp(12.))
                .child(hint("Variables use the selected environment. HTTP scripts are not run for RabbitMQ operations.", cx))
                .child(scroll("rmq-vars-scroll", self.variables.clone().into_any_element()))
                .into_any_element(),
            Section::Docs => code_box(Editor::new(&self.docs).bordered(false).h_full(), cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .child(section_tab_strip("rmq-sections", tabs, cx))
            .child(div().id("rmq-operation-content").flex_1().min_h_0().overflow_y_scroll().p(dp(16.)).child(content))
            .into_any_element()
    }

    fn render_publish(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .gap(dp(12.))
            .child(labelled_input("Exchange", self.input("exchange"), cx))
            .child(labelled_input("Routing key", self.input("routingKey"), cx))
            .child(labelled_input("Content type", self.input("contentType"), cx))
            .child(labelled_input("Correlation ID", self.input("correlationId"), cx))
            .child(labelled_input("Reply to", self.input("replyTo"), cx))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(dp(12.))
                    .child(Checkbox::new("rmq-persistent").label("Persistent message").checked(self.persistent).on_click(cx.listener(
                        |tab, checked, _, cx| {
                            tab.persistent = *checked;
                            cx.emit(RabbitMqChanged);
                            cx.notify();
                        },
                    )))
                    .child(div().flex_1())
                    .child(
                        Button::new("rmq-publish")
                            .primary()
                            .label("Publish")
                            .disabled(!self.connected || self.busy)
                            .on_click(cx.listener(|tab, _, window, cx| tab.send(window, cx))),
                    ),
            )
            .child(hint("Payload", cx))
            .child(div().flex_1().min_h(dp(100.)).child(code_box(Editor::new(&self.body).bordered(false).h_full(), cx)))
            .into_any_element()
    }

    fn render_consume(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_pending = self.messages.iter().any(|message| message.state == kestrel_core::rabbitmq::MessageState::Pending);
        let tab = cx.entity().downgrade();
        let prefetch = segmented(
            "rmq-prefetch",
            [1, 10, 50, 100].into_iter().map(|value| (value, value.to_string())).collect(),
            &self.prefetch,
            move |prefetch, _, cx| {
                tab.update(cx, |tab, cx| {
                    tab.prefetch = prefetch;
                    cx.emit(RabbitMqChanged);
                    cx.notify();
                })
                .report_unless_gone("change RabbitMQ prefetch");
            },
            cx,
        );
        v_flex()
            .gap(dp(16.))
            .child(labelled_input("Queue", self.input("queue"), cx))
            .child(h_flex().gap(dp(12.)).child(hint("Prefetch", cx)).child(prefetch))
            .child(hint("Fetch one message or listen for new deliveries. Acknowledge, requeue or reject each message in the journal.", cx))
            .child(hint(
                "Ack removes a message. Requeue returns it. Reject discards it or sends it to a configured dead-letter exchange.",
                cx,
            ))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(dp(8.))
                    .child(
                        Button::new("rmq-get")
                            .label("Get message")
                            .disabled(!self.connected || self.busy || self.subscribing)
                            .on_click(cx.listener(|tab, _, window, cx| tab.send(window, cx))),
                    )
                    .child(
                        Button::new("rmq-subscribe")
                            .primary()
                            .label(if self.subscribing { "Stop listening" } else { "Listen" })
                            .disabled(!self.connected || self.busy || (!self.subscribing && has_pending))
                            .on_click(cx.listener(|tab, _, window, cx| tab.toggle_subscription(window, cx))),
                    ),
            )
            .child(hint("Unacknowledged deliveries are returned when you disconnect or close this tab.", cx))
            .child(hint(
                "Settle pending messages before starting a new subscription. Queue and prefetch changes apply when you start listening.",
                cx,
            ))
            .into_any_element()
    }

    fn render_connection_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut form = v_flex().gap(dp(14.));
        for (key, label) in [("vhost", "Virtual host"), ("username", "Username"), ("password", "Password variable")] {
            form =
                form.child(v_flex().gap(dp(6.)).child(hint(label, cx)).child(Input::new(self.input(key)).disabled(self.client.is_some())));
        }
        scroll(
            "rmq-connection-settings",
            form.child(hint("Store the password in .env as RABBITMQ_PASSWORD and reference {{process.env.RABBITMQ_PASSWORD}}.", cx))
                .child(hint("Use amqps:// for TLS with a trusted server certificate. Disconnect before changing connection settings.", cx))
                .into_any_element(),
        )
    }
}

impl Render for RabbitMqTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        v_flex()
            .size_full()
            .child(self.render_connection_status(cx))
            .when_some(self.error.clone(), |element, error| {
                element.child(
                    div()
                        .mx(dp(16.))
                        .mb(dp(12.))
                        .p(dp(12.))
                        .rounded(dp(10.))
                        .bg(palette.tint(palette.danger))
                        .child(crate::ui::error_message("rmq-copy-error", &error, cx)),
                )
            })
            .child(
                div().flex_1().min_h_0().border_t_1().border_color(palette.border_soft).child(
                    h_resizable("rabbitmq-panels").child(resizable_panel().child(self.render_operations(cx))).child(
                        resizable_panel()
                            .child(div().size_full().border_l_1().border_color(palette.border_soft).child(self.render_messages(cx))),
                    ),
                ),
            )
    }
}
