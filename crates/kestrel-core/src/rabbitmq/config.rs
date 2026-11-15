use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::format::{Extra, KeyValue};
use crate::vars::Scope;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RabbitMq {
    pub url: String,
    pub vhost: String,
    pub username: String,
    pub password: String,
    pub operation: Operation,
    pub exchange: String,
    pub routing_key: String,
    pub queue: String,
    pub body: String,
    pub content_type: String,
    pub correlation_id: String,
    pub reply_to: String,
    pub persistent: bool,
    pub prefetch: u16,
    pub headers: Vec<KeyValue>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for RabbitMq {
    fn default() -> Self {
        Self {
            url: "amqp://localhost:5672".into(),
            vhost: "/".into(),
            username: "guest".into(),
            password: "{{process.env.RABBITMQ_PASSWORD}}".into(),
            operation: Operation::Publish,
            exchange: String::new(),
            routing_key: String::new(),
            queue: String::new(),
            body: "{}".into(),
            content_type: "application/json".into(),
            correlation_id: String::new(),
            reply_to: String::new(),
            persistent: true,
            prefetch: 10,
            headers: Vec::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    #[default]
    Publish,
    Get,
}

impl RabbitMq {
    pub fn validate_saved_connection(&self) -> Result<()> {
        ensure!(self.password.is_empty() || is_secret_reference(&self.password), "Use a variable reference for the RabbitMQ password");
        ensure!(!self.url.contains('@'), "Keep credentials out of the broker URL; use the username and password fields");
        if !self.url.contains("{{") {
            let url = url::Url::parse(&self.url)?;
            ensure!(
                url.username().is_empty() && url.password().is_none(),
                "Keep credentials out of the broker URL; use the username and password fields"
            );
        }
        Ok(())
    }

    pub fn resolve_connection(&self, scope: &Scope) -> Result<Self> {
        let mut resolved = self.clone();
        for (label, value) in [
            ("broker URL", &mut resolved.url),
            ("virtual host", &mut resolved.vhost),
            ("username", &mut resolved.username),
            ("password", &mut resolved.password),
        ] {
            *value = interpolate(scope, value, label)?;
        }
        resolved.connection_uri()?;
        Ok(resolved)
    }

    pub fn resolve(&self, scope: &Scope) -> Result<Self> {
        let mut resolved = self.resolve_connection(scope)?;
        if self.operation == Operation::Get {
            resolved.queue = interpolate(scope, &resolved.queue, "queue")?;
            return Ok(resolved);
        }
        for (label, value) in [
            ("exchange", &mut resolved.exchange),
            ("routing key", &mut resolved.routing_key),
            ("payload", &mut resolved.body),
            ("content type", &mut resolved.content_type),
            ("correlation ID", &mut resolved.correlation_id),
            ("reply queue", &mut resolved.reply_to),
        ] {
            *value = interpolate(scope, value, label)?;
        }
        for header in resolved.headers.iter_mut().filter(|header| header.enabled()) {
            header.name = interpolate(scope, &header.name, "header name")?;
            header.value = interpolate(scope, &header.value, "header value")?;
        }
        Ok(resolved)
    }

    pub fn connection_uri(&self) -> Result<String> {
        let mut url = url::Url::parse(&self.url).map_err(|_| anyhow::anyhow!("Enter a valid amqp:// or amqps:// broker URL"))?;
        ensure!(matches!(url.scheme(), "amqp" | "amqps") && url.host_str().is_some(), "Use an amqp:// or amqps:// broker URL");
        ensure!(url.username().is_empty() && url.password().is_none(), "Keep credentials out of the broker URL");
        ensure!(url.path().is_empty() || url.path() == "/", "Set the virtual host in its own field");
        ensure!(url.query().is_none() && url.fragment().is_none(), "Broker URLs cannot contain a query or fragment");
        url.set_username(&self.username).map_err(|_| anyhow::anyhow!("Invalid broker username"))?;
        url.set_password(Some(&self.password)).map_err(|_| anyhow::anyhow!("Invalid broker password"))?;
        let vhost = percent_encoding::utf8_percent_encode(&self.vhost, percent_encoding::NON_ALPHANUMERIC);
        url.set_path(&format!("/{vhost}"));
        Ok(url.into())
    }
}

fn is_secret_reference(value: &str) -> bool {
    let Some(name) = value.strip_prefix("{{").and_then(|value| value.strip_suffix("}}")) else { return false };
    !name.is_empty() && !name.contains(['{', '}']) && name.trim() == name
}

fn interpolate(scope: &Scope, value: &str, label: &str) -> Result<String> {
    let missing: Vec<_> = crate::vars::names(value).into_iter().filter(|name| scope.get(name).is_none()).collect();
    ensure!(missing.is_empty(), "Variables not set in {label}: {}", missing.join(", "));
    Ok(scope.interpolate(value))
}
