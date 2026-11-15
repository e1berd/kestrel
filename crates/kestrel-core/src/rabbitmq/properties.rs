use anyhow::{Result, ensure};
use lapin::BasicProperties;
use lapin::types::{AMQPValue, FieldTable};

use super::RabbitMq;

pub(super) fn build(config: &RabbitMq) -> Result<BasicProperties> {
    let mut headers = FieldTable::default();
    for header in config.headers.iter().filter(|header| header.enabled()) {
        ensure!(header.name.len() <= 255, "Message header names cannot exceed 255 bytes");
        headers.insert(header.name.clone().into(), AMQPValue::LongString(header.value.clone().into()));
    }
    for value in [&config.content_type, &config.correlation_id, &config.reply_to] {
        ensure!(value.len() <= 255, "Message properties cannot exceed 255 bytes");
    }
    let mut properties = BasicProperties::default().with_headers(headers).with_delivery_mode(if config.persistent { 2 } else { 1 });
    if !config.content_type.is_empty() {
        properties = properties.with_content_type(config.content_type.clone().into());
    }
    if !config.correlation_id.is_empty() {
        properties = properties.with_correlation_id(config.correlation_id.clone().into());
    }
    if !config.reply_to.is_empty() {
        properties = properties.with_reply_to(config.reply_to.clone().into());
    }
    Ok(properties)
}

pub(super) fn describe(properties: &BasicProperties) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for (name, value) in [
        ("Content type", properties.content_type()),
        ("Content encoding", properties.content_encoding()),
        ("Correlation ID", properties.correlation_id()),
        ("Reply to", properties.reply_to()),
        ("Message ID", properties.message_id()),
        ("Type", properties.kind()),
        ("App ID", properties.app_id()),
        ("Expiration", properties.expiration()),
    ] {
        if let Some(value) = value {
            rows.push((name.into(), value.to_string()));
        }
    }
    if let Some(mode) = properties.delivery_mode() {
        rows.push(("Delivery mode".into(), mode.to_string()));
    }
    if let Some(headers) = properties.headers() {
        for (name, value) in headers.inner() {
            let value = match value {
                AMQPValue::LongString(value) => String::from_utf8_lossy(value.as_bytes()).into_owned(),
                other => format!("{other:?}"),
            };
            rows.push((name.to_string(), value));
        }
    }
    rows
}
