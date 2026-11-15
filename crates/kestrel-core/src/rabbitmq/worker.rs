use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Result, ensure};
use futures::channel::mpsc::Sender;
use futures::{SinkExt as _, StreamExt as _};
use lapin::message::Delivery;
use lapin::options::{
    BasicAckOptions, BasicCancelOptions, BasicConsumeOptions, BasicGetOptions, BasicNackOptions, BasicPublishOptions, BasicQosOptions,
    ConfirmSelectOptions,
};
use lapin::{Channel, Connection, ConnectionProperties, Consumer};

use super::{Command, Event, Message, MessageState, RabbitMq, Settlement};

const TIMEOUT: Duration = Duration::from_secs(15);

pub(super) async fn run(config: RabbitMq, mut commands: tokio::sync::mpsc::Receiver<Command>, mut events: Sender<Event>) {
    if let Err(error) = run_connected(config, &mut commands, &mut events).await {
        crate::report::report_error("run a RabbitMQ connection", &error);
        if events.send(Event::Error(format!("{error:#}"))).await.is_err() {
            return;
        }
    }
    if let Err(error) = events.send(Event::Disconnected).await
        && !error.is_disconnected()
    {
        crate::report::report_error("show RabbitMQ disconnection", error);
    }
}

async fn run_connected(config: RabbitMq, commands: &mut tokio::sync::mpsc::Receiver<Command>, events: &mut Sender<Event>) -> Result<()> {
    let uri = config.connection_uri()?;
    let connection =
        tokio::time::timeout(TIMEOUT, Connection::connect(&uri, ConnectionProperties::default().with_connection_name("Kestrel".into())))
            .await
            .map_err(|_| anyhow::anyhow!("RabbitMQ connection timed out after 15 seconds"))?
            .map_err(|_| {
                anyhow::anyhow!("Could not connect to RabbitMQ; check the broker, credentials, virtual host and TLS certificate")
            })?;
    let channel = tokio::time::timeout(TIMEOUT, connection.create_channel()).await??;
    tokio::time::timeout(TIMEOUT, channel.confirm_select(ConfirmSelectOptions::default())).await??;
    events.send(Event::Connected).await?;
    let mut consumer = None;
    let mut pending = HashSet::new();
    let mut health = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = health.tick() => {
                ensure!(channel.status().connected(), "RabbitMQ closed the channel; reconnect to continue");
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                let result = tokio::time::timeout(TIMEOUT, execute(command, &channel, &mut consumer, &mut pending)).await;
                match result {
                    Ok(Ok(event)) => events.send(event).await?,
                    Ok(Err(error)) => {
                        if !channel.status().connected() {
                            return Err(error);
                        }
                        crate::report::report_error("execute a RabbitMQ operation", &error);
                        events.send(Event::Error(format!("{error:#}"))).await?;
                    }
                    Err(_) => anyhow::bail!("RabbitMQ operation timed out; disconnecting to release pending messages"),
                }
            }
            delivery = next_delivery(&mut consumer) => {
                match delivery {
                    Some(Ok(delivery)) => {
                        pending.insert(delivery.delivery_tag);
                        events.send(Event::Message(received(delivery))).await?;
                    }
                    Some(Err(error)) => return Err(error.into()),
                    None => {
                        consumer = None;
                        events.send(Event::Stopped).await?;
                    }
                }
            }
        }
        ensure!(channel.status().connected(), "RabbitMQ closed the channel; reconnect to continue");
    }
    tokio::time::timeout(TIMEOUT, connection.close(200, "Goodbye".into())).await??;
    Ok(())
}

async fn next_delivery(consumer: &mut Option<Consumer>) -> Option<lapin::Result<Delivery>> {
    match consumer {
        Some(consumer) => consumer.next().await,
        None => std::future::pending().await,
    }
}

async fn execute(command: Command, channel: &Channel, consumer: &mut Option<Consumer>, pending: &mut HashSet<u64>) -> Result<Event> {
    match command {
        Command::Publish(config) => publish(channel, &config).await,
        Command::Get(queue) => {
            ensure!(consumer.is_none(), "Stop the subscription before fetching a message");
            ensure!(pending.len() < 100, "Acknowledge or requeue pending messages before fetching more");
            ensure!(!queue.is_empty() && queue.len() <= 255, "Enter a queue name of up to 255 bytes");
            match channel.basic_get(queue.into(), BasicGetOptions { no_ack: false }).await? {
                Some(message) => {
                    pending.insert(message.delivery.delivery_tag);
                    Ok(Event::Message(received(message.delivery)))
                }
                None => Ok(Event::Empty),
            }
        }
        Command::Subscribe { queue, prefetch } => {
            ensure!(consumer.is_none(), "A subscription is already running");
            ensure!(pending.is_empty(), "Settle pending messages before starting another subscription");
            ensure!(!queue.is_empty() && queue.len() <= 255, "Enter a queue name of up to 255 bytes");
            ensure!((1..=100).contains(&prefetch), "Prefetch must be between 1 and 100");
            channel.basic_qos(prefetch, BasicQosOptions::default()).await?;
            *consumer = Some(channel.basic_consume(queue.into(), "".into(), BasicConsumeOptions::default(), Default::default()).await?);
            Ok(Event::Subscribed)
        }
        Command::Stop => {
            if let Some(active) = consumer.as_ref() {
                channel.basic_cancel(active.tag(), BasicCancelOptions::default()).await?;
                return Ok(Event::Stopping);
            }
            Ok(Event::Stopped)
        }
        Command::Settle { delivery_tag, settlement } => {
            ensure!(pending.contains(&delivery_tag), "The message is no longer pending on this connection");
            let state = match settlement {
                Settlement::Ack => {
                    channel.basic_ack(delivery_tag, BasicAckOptions::default()).await?;
                    MessageState::Acknowledged
                }
                Settlement::Requeue | Settlement::Reject => {
                    let requeue = matches!(settlement, Settlement::Requeue);
                    channel.basic_nack(delivery_tag, BasicNackOptions { multiple: false, requeue }).await?;
                    if requeue { MessageState::Requeued } else { MessageState::Rejected }
                }
            };
            pending.remove(&delivery_tag);
            Ok(Event::Settled { delivery_tag, state })
        }
    }
}

async fn publish(channel: &Channel, config: &RabbitMq) -> Result<Event> {
    ensure!(config.exchange.len() <= 255 && config.routing_key.len() <= 255, "Exchange and routing key cannot exceed 255 bytes");
    let properties = super::properties::build(config)?;
    let confirmation = channel
        .basic_publish(
            config.exchange.clone().into(),
            config.routing_key.clone().into(),
            BasicPublishOptions { mandatory: true, immediate: false },
            config.body.as_bytes(),
            properties.clone(),
        )
        .await?
        .await?;
    ensure!(confirmation.is_ack(), "RabbitMQ did not confirm the publication");
    ensure!(confirmation.take_message().is_none(), "The message was returned as unroutable; check the exchange, routing key and bindings");
    Ok(Event::Message(Message {
        delivery_tag: None,
        exchange: config.exchange.clone(),
        routing_key: config.routing_key.clone(),
        redelivered: false,
        body: config.body.as_bytes()[..config.body.len().min(256 * 1024)].to_vec(),
        body_bytes: config.body.len(),
        properties: super::properties::describe(&properties),
        state: MessageState::Published,
    }))
}

fn received(mut delivery: Delivery) -> Message {
    let body_bytes = delivery.data.len();
    delivery.data.truncate(256 * 1024);
    Message {
        delivery_tag: Some(delivery.delivery_tag),
        exchange: delivery.exchange.to_string(),
        routing_key: delivery.routing_key.to_string(),
        redelivered: delivery.redelivered,
        body: delivery.data,
        body_bytes,
        properties: super::properties::describe(&delivery.properties),
        state: MessageState::Pending,
    }
}
