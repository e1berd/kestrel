use anyhow::{Result, ensure};
use futures::channel::mpsc;

use super::RabbitMq;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Message {
    pub delivery_tag: Option<u64>,
    pub exchange: String,
    pub routing_key: String,
    pub redelivered: bool,
    pub body: Vec<u8>,
    pub body_bytes: usize,
    pub properties: Vec<(String, String)>,
    pub state: MessageState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MessageState {
    Pending,
    Published,
    Acknowledged,
    Requeued,
    Rejected,
    Released,
}

impl MessageState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Awaiting ack",
            Self::Published => "Confirmed",
            Self::Acknowledged => "Acknowledged",
            Self::Requeued => "Requeued",
            Self::Rejected => "Rejected",
            Self::Released => "Returned on disconnect",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Settlement {
    Ack,
    Requeue,
    Reject,
}

pub enum Command {
    Publish(Box<RabbitMq>),
    Get(String),
    Subscribe { queue: String, prefetch: u16 },
    Stop,
    Settle { delivery_tag: u64, settlement: Settlement },
}

pub enum Event {
    Connected,
    Message(Message),
    Empty,
    Subscribed,
    Stopping,
    Stopped,
    Settled { delivery_tag: u64, state: MessageState },
    Error(String),
    Disconnected,
}

pub struct Client {
    commands: tokio::sync::mpsc::Sender<Command>,
    worker: tokio::task::JoinHandle<()>,
}

impl Client {
    pub fn connect(config: RabbitMq) -> (Self, mpsc::Receiver<Event>) {
        let (commands, receiver) = tokio::sync::mpsc::channel(16);
        let (events, incoming) = mpsc::channel(32);
        let worker = crate::runtime().spawn(super::worker::run(config, receiver, events));
        (Self { commands, worker }, incoming)
    }

    pub fn send(&self, command: Command) -> Result<()> {
        ensure!(!self.worker.is_finished(), "The RabbitMQ connection is closed; reconnect to continue");
        self.commands.try_send(command).map_err(|_| anyhow::anyhow!("The RabbitMQ connection is busy or closed; try again"))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
