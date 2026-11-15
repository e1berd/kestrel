use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::mpsc::Receiver;
use kestrel_core::rabbitmq::{Client, Command, Event, Message, MessageState, RabbitMq, Settlement};
use lapin::options::{QueueDeclareOptions, QueueDeleteOptions};
use lapin::{Connection, ConnectionProperties};

#[tokio::test]
#[ignore = "requires a RabbitMQ broker; configure KESTREL_RABBITMQ_URL, KESTREL_RABBITMQ_USERNAME and KESTREL_RABBITMQ_PASSWORD"]
async fn publishes_fetches_requeues_acknowledges_subscribes_and_reports_unroutable_messages() {
    let queue_name = format!("kestrel-test-{}", uuid::Uuid::new_v4());
    let config = RabbitMq {
        url: std::env::var("KESTREL_RABBITMQ_URL").unwrap_or_else(|_| "amqp://localhost:5672".into()),
        username: std::env::var("KESTREL_RABBITMQ_USERNAME").unwrap_or_else(|_| "guest".into()),
        password: std::env::var("KESTREL_RABBITMQ_PASSWORD").unwrap_or_else(|_| "guest".into()),
        routing_key: queue_name.clone(),
        body: "{\"event\":\"order.created\"}".into(),
        ..Default::default()
    };
    let owner = Connection::connect(&config.connection_uri().unwrap(), ConnectionProperties::default()).await.unwrap();
    let channel = owner.create_channel().await.unwrap();
    channel.queue_declare(queue_name.clone().into(), QueueDeclareOptions::default(), Default::default()).await.unwrap();

    let (client, mut events) = Client::connect(config.clone());
    assert!(matches!(next_event(&mut events).await, Event::Connected));
    client.send(Command::Publish(Box::new(config.clone()))).unwrap();
    let published = next_message(&mut events).await;
    assert_eq!(published.state, MessageState::Published);
    assert_eq!(published.body, config.body.as_bytes());

    client.send(Command::Get(queue_name.clone())).unwrap();
    let received = next_message(&mut events).await;
    assert_eq!(received.body, config.body.as_bytes());
    assert_eq!(received.state, MessageState::Pending);
    client.send(Command::Settle { delivery_tag: received.delivery_tag.unwrap(), settlement: Settlement::Requeue }).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Settled { state: MessageState::Requeued, .. }));

    client.send(Command::Get(queue_name.clone())).unwrap();
    let redelivery = next_message(&mut events).await;
    assert!(redelivery.redelivered);
    client.send(Command::Settle { delivery_tag: redelivery.delivery_tag.unwrap(), settlement: Settlement::Ack }).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Settled { state: MessageState::Acknowledged, .. }));
    client.send(Command::Get(queue_name.clone())).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Empty));

    let unroutable = RabbitMq { routing_key: format!("{queue_name}-absent"), ..config.clone() };
    client.send(Command::Publish(Box::new(unroutable))).unwrap();
    match next_event(&mut events).await {
        Event::Error(error) => assert!(error.contains("unroutable")),
        _ => panic!("expected an unroutable publication error"),
    }

    client.send(Command::Subscribe { queue: queue_name.clone(), prefetch: 1 }).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Subscribed));
    client.send(Command::Publish(Box::new(config))).unwrap();
    let mut delivered = None;
    for _ in 0..2 {
        let message = next_message(&mut events).await;
        if message.state == MessageState::Pending {
            delivered = Some(message);
        }
    }
    let delivered = delivered.expect("a subscribed delivery");
    client.send(Command::Stop).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Stopping));
    assert!(matches!(next_event(&mut events).await, Event::Stopped));
    client.send(Command::Settle { delivery_tag: delivered.delivery_tag.unwrap(), settlement: Settlement::Reject }).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Settled { state: MessageState::Rejected, .. }));
    client.send(Command::Get(queue_name.clone())).unwrap();
    assert!(matches!(next_event(&mut events).await, Event::Empty));
    drop(client);

    channel.queue_delete(queue_name.into(), QueueDeleteOptions::default()).await.unwrap();
    owner.close(200, "Test finished".into()).await.unwrap();
}

async fn next_event(events: &mut Receiver<Event>) -> Event {
    tokio::time::timeout(Duration::from_secs(20), events.next()).await.expect("RabbitMQ event timeout").expect("RabbitMQ event")
}

async fn next_message(events: &mut Receiver<Event>) -> Message {
    match next_event(events).await {
        Event::Message(message) => message,
        Event::Error(error) => panic!("RabbitMQ operation failed: {error}"),
        _ => panic!("expected a RabbitMQ message"),
    }
}
