use anyhow::{Result, bail, ensure};
use futures::StreamExt as _;

use super::{Client, Command, Event, Message, MessageState, Operation, Settlement};
use crate::{Outcome, Plan, Session};

pub(crate) async fn send(plan: &Plan, session: &Session) -> Outcome {
    match execute(plan, session).await {
        Ok(message) => Outcome { rabbitmq: message, ..Default::default() },
        Err(error) => {
            crate::report::report_error("execute a RabbitMQ operation", &error);
            Outcome { error: Some(format!("{error:#}")), ..Default::default() }
        }
    }
}

async fn execute(plan: &Plan, session: &Session) -> Result<Option<Message>> {
    ensure!(
        ["before-request", "after-response", "tests"].iter().all(|kind| plan.scripts(kind).is_empty()),
        "HTTP scripts are not supported for RabbitMQ operations"
    );
    let config =
        plan.file.rabbitmq.as_ref().ok_or_else(|| anyhow::anyhow!("No RabbitMQ operation in this file"))?.resolve(&plan.scope(session))?;
    let operation = config.operation;
    let (client, mut events) = Client::connect(config.clone());
    let mut received = None;
    while let Some(event) = events.next().await {
        match event {
            Event::Connected => {
                let command = match operation {
                    Operation::Publish => Command::Publish(Box::new(config.clone())),
                    Operation::Get => Command::Get(config.queue.clone()),
                };
                client.send(command)?;
            }
            Event::Message(message) if message.state == MessageState::Published => return Ok(Some(message)),
            Event::Message(message) => {
                let delivery_tag = message.delivery_tag.ok_or_else(|| anyhow::anyhow!("The message has no delivery tag"))?;
                client.send(Command::Settle { delivery_tag, settlement: Settlement::Requeue })?;
                received = Some(message);
            }
            Event::Settled { state, .. } => {
                if let Some(message) = &mut received {
                    message.state = state;
                }
                return Ok(received);
            }
            Event::Empty => return Ok(None),
            Event::Error(error) => bail!("{error}"),
            Event::Disconnected => bail!("RabbitMQ disconnected before the operation completed"),
            Event::Subscribed | Event::Stopping | Event::Stopped => {}
        }
    }
    bail!("RabbitMQ disconnected before the operation completed")
}
