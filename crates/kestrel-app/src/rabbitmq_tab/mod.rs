mod connection;
mod messages;
mod view;

use std::path::PathBuf;

use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use gpui_component::input::{EditorState, InputEvent, InputState};
use kestrel_core::rabbitmq::{Client, Message, Operation, RabbitMq};

use crate::kv_table::{KvChanged, KvTable};
use crate::project::Project;

pub struct RabbitMqChanged;

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Publish,
    Consume,
    Connection,
    Headers,
    Variables,
    Docs,
}

pub struct RabbitMqTab {
    project: Entity<Project>,
    path: PathBuf,
    base: RabbitMq,
    inputs: Vec<(&'static str, Entity<InputState>)>,
    body: Entity<EditorState>,
    headers: Entity<KvTable>,
    variables: Entity<KvTable>,
    docs: Entity<EditorState>,
    section: Section,
    persistent: bool,
    prefetch: u16,
    operation: Operation,
    client: Option<Client>,
    events: Option<Task<()>>,
    connected: bool,
    busy: bool,
    subscribing: bool,
    status: String,
    error: Option<String>,
    messages: Vec<Message>,
    selected: Option<usize>,
    message_body: Entity<EditorState>,
    show_properties: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RabbitMqChanged> for RabbitMqTab {}

impl RabbitMqTab {
    pub fn new(
        project: Entity<Project>,
        path: PathBuf,
        config: RabbitMq,
        variables: Entity<KvTable>,
        docs: Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let inputs: Vec<_> = [
            ("url", config.url.clone(), "amqp://localhost:5672"),
            ("vhost", config.vhost.clone(), "/"),
            ("username", config.username.clone(), "guest"),
            ("password", config.password.clone(), "{{process.env.RABBITMQ_PASSWORD}}"),
            ("exchange", config.exchange.clone(), "Empty for the default exchange"),
            ("routingKey", config.routing_key.clone(), "orders.created"),
            ("queue", config.queue.clone(), "orders"),
            ("contentType", config.content_type.clone(), "application/json"),
            ("correlationId", config.correlation_id.clone(), "Optional correlation ID"),
            ("replyTo", config.reply_to.clone(), "Optional reply queue"),
        ]
        .into_iter()
        .map(|(key, value, placeholder)| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value));
            (key, input)
        })
        .collect();
        let body = editor("json", config.body.clone(), window, cx);
        let headers = cx.new(|cx| KvTable::new("rmq-headers", &config.headers, window, cx).placeholders("Header", "Value"));
        let mut subscriptions = Vec::new();
        for (_, input) in &inputs {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.emit(RabbitMqChanged);
                    cx.notify();
                }
            }));
        }
        subscriptions.push(cx.subscribe(&body, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.emit(RabbitMqChanged);
            }
        }));
        subscriptions.push(cx.subscribe(&headers, |_, _, _: &KvChanged, cx| cx.emit(RabbitMqChanged)));
        Self {
            project,
            path,
            persistent: config.persistent,
            prefetch: config.prefetch,
            operation: config.operation,
            section: if config.operation == Operation::Get { Section::Consume } else { Section::Publish },
            base: config,
            inputs,
            body,
            headers,
            variables,
            docs,
            client: None,
            events: None,
            connected: false,
            busy: false,
            subscribing: false,
            status: "Disconnected".into(),
            error: None,
            messages: Vec::new(),
            selected: None,
            message_body: editor("text", String::new(), window, cx),
            show_properties: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn config(&self, cx: &App) -> RabbitMq {
        RabbitMq {
            url: self.value("url", cx),
            vhost: self.value("vhost", cx),
            username: self.value("username", cx),
            password: self.value("password", cx),
            exchange: self.value("exchange", cx),
            routing_key: self.value("routingKey", cx),
            queue: self.value("queue", cx),
            content_type: self.value("contentType", cx),
            correlation_id: self.value("correlationId", cx),
            reply_to: self.value("replyTo", cx),
            body: self.body.read(cx).value().to_string(),
            headers: self.headers.read(cx).items(cx),
            persistent: self.persistent,
            prefetch: self.prefetch,
            operation: self.operation,
            extra: self.base.extra.clone(),
        }
    }

    pub fn relocate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.disconnect(cx);
        self.path = path;
    }

    fn input(&self, key: &str) -> &Entity<InputState> {
        &self.inputs.iter().find(|(name, _)| *name == key).expect("a RabbitMQ input field").1
    }

    fn value(&self, key: &str, cx: &App) -> String {
        self.input(key).read(cx).value().to_string()
    }
}

fn editor(language: &'static str, text: String, window: &mut Window, cx: &mut Context<RabbitMqTab>) -> Entity<EditorState> {
    cx.new(|cx| EditorState::new(window, cx).language(language).line_number(true).soft_wrap(true).default_value(text))
}
