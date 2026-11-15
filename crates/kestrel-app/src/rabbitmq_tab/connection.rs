use anyhow::{Result, ensure};
use futures::StreamExt as _;
use gpui::{App, Context, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use kestrel_core::Plan;
use kestrel_core::format::Runtime;
use kestrel_core::rabbitmq::{Client, Command, Event, MessageState, Operation, RabbitMq, Settlement};

use super::{RabbitMqChanged, RabbitMqTab, Section};
use crate::ui::ReportUnlessGone as _;

impl RabbitMqTab {
    fn resolved(&self, connection_only: bool, cx: &App) -> Result<RabbitMq> {
        let project = self.project.read(cx);
        let mut file = project
            .collection
            .request(&self.path)
            .ok_or_else(|| anyhow::anyhow!("The operation is no longer in the collection"))?
            .doc
            .data
            .clone();
        file.runtime.get_or_insert_with(Runtime::default).variables = self.variables.read(cx).items(cx);
        let plan = Plan::with_file(&project.collection, &self.path, file, project.env.as_deref())?;
        let config = self.config(cx);
        let scope = plan.scope(&project.session);
        if connection_only { config.resolve_connection(&scope) } else { config.resolve(&scope) }
    }

    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.client = None;
        self.events = None;
        self.connected = false;
        self.busy = false;
        self.subscribing = false;
        self.status = "Disconnected".into();
        for message in &mut self.messages {
            if message.state == MessageState::Pending {
                message.state = MessageState::Released;
            }
        }
        cx.notify();
    }

    pub(super) fn toggle_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.client.is_some() {
            self.disconnect(cx);
            return;
        }
        let config = match self.resolved(true, cx) {
            Ok(config) => config,
            Err(error) => return self.show_error(error, window, cx),
        };
        let (client, mut incoming) = Client::connect(config);
        self.client = Some(client);
        self.busy = true;
        self.error = None;
        self.status = "Connecting…".into();
        self.events = Some(cx.spawn_in(window, async move |tab, cx| {
            while let Some(event) = incoming.next().await {
                let updated = tab.update_in(cx, |tab, window, cx| tab.apply_event(event, window, cx));
                if let Err(error) = updated {
                    Err::<(), _>(error).report_unless_gone("show a RabbitMQ event");
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.connected {
            self.toggle_connection(window, cx);
            return;
        }
        if self.busy {
            return;
        }
        let result = self.resolved(false, cx).and_then(|config| {
            let command =
                if self.operation == Operation::Publish { Command::Publish(Box::new(config)) } else { Command::Get(config.queue) };
            self.dispatch(command)
        });
        match result {
            Ok(()) => {
                self.busy = true;
                self.error = None;
                self.status = if self.operation == Operation::Publish { "Publishing…" } else { "Fetching a message…" }.into();
            }
            Err(error) => self.show_error(error, window, cx),
        }
        cx.notify();
    }

    pub(super) fn toggle_subscription(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = if self.subscribing {
            self.dispatch(Command::Stop)
        } else {
            self.resolved(false, cx).and_then(|config| self.dispatch(Command::Subscribe { queue: config.queue, prefetch: config.prefetch }))
        };
        match result {
            Ok(()) => self.busy = true,
            Err(error) => self.show_error(error, window, cx),
        }
        cx.notify();
    }

    pub(super) fn settle(&mut self, settlement: Settlement, window: &mut Window, cx: &mut Context<Self>) {
        let Some(message) = self.selected.and_then(|index| self.messages.get(index)) else { return };
        if message.state != MessageState::Pending {
            return;
        }
        let Some(delivery_tag) = message.delivery_tag else { return };
        match self.dispatch(Command::Settle { delivery_tag, settlement }) {
            Ok(()) => self.busy = true,
            Err(error) => self.show_error(error, window, cx),
        }
        cx.notify();
    }

    fn dispatch(&self, command: Command) -> Result<()> {
        ensure!(self.connected, "Connect to RabbitMQ first");
        self.client.as_ref().ok_or_else(|| anyhow::anyhow!("Connect to RabbitMQ first"))?.send(command)
    }

    fn apply_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(&event, Event::Message(message) if message.state == MessageState::Pending && self.subscribing) {
            self.busy = false;
        }
        match event {
            Event::Connected => {
                self.connected = true;
                self.status = "Connected".into();
            }
            Event::Message(message) => {
                self.status = if message.state == MessageState::Published { "Publication confirmed" } else { "Message received" }.into();
                self.append_message(message, window, cx);
            }
            Event::Empty => self.status = "The queue is empty".into(),
            Event::Subscribed => {
                self.subscribing = true;
                self.status = "Listening for messages".into();
            }
            Event::Stopping => {
                self.busy = true;
                self.status = "Stopping subscription…".into();
            }
            Event::Stopped => {
                self.subscribing = false;
                self.status = "Subscription stopped".into();
            }
            Event::Settled { delivery_tag, state } => {
                if let Some(message) = self
                    .messages
                    .iter_mut()
                    .find(|message| message.delivery_tag == Some(delivery_tag) && message.state == MessageState::Pending)
                {
                    message.state = state;
                }
                self.status = state.label().into();
            }
            Event::Error(error) => self.show_error(anyhow::anyhow!(error), window, cx),
            Event::Disconnected => {
                self.client = None;
                self.connected = false;
                self.subscribing = false;
                self.status = "Disconnected".into();
                for message in &mut self.messages {
                    if message.state == MessageState::Pending {
                        message.state = MessageState::Released;
                    }
                }
            }
        }
        cx.notify();
    }

    pub(super) fn select_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.operation = match section {
            Section::Publish => Operation::Publish,
            Section::Consume => Operation::Get,
            _ => self.operation,
        };
        cx.emit(RabbitMqChanged);
        cx.notify();
    }

    fn show_error(&mut self, error: anyhow::Error, window: &mut Window, cx: &mut Context<Self>) {
        kestrel_core::report::report_error("work with RabbitMQ", &error);
        self.error = Some(format!("{error:#}"));
        window.push_notification(Notification::error(format!("{error:#}")), cx);
        cx.notify();
    }
}
