use std::path::PathBuf;

use kestrel_core::format::RequestFile;
use kestrel_core::rabbitmq::{Operation, RabbitMq};
use kestrel_core::vars::Scope;
use kestrel_core::{Collection, Document};

#[test]
fn rabbitmq_documents_preserve_unknown_fields_and_only_change_the_edited_value() {
    let mut file = RequestFile::default();
    file.info.name = "Publish order".into();
    file.info.kind = "rabbitmq".into();
    let mut config = RabbitMq { routing_key: "orders.created".into(), ..Default::default() };
    config.extra.insert("futureSetting".into(), "preserved".into());
    file.rabbitmq = Some(config);
    let document = Document::new(PathBuf::from("rabbitmq.yml"), file, true);
    let source = document.render().unwrap();
    let mut reopened = Document::<RequestFile>::parse(&document.path, &source, true).unwrap();
    assert_eq!(reopened.render().unwrap(), source);
    reopened.data.rabbitmq.as_mut().unwrap().routing_key = "orders.updated".into();
    assert_eq!(reopened.render().unwrap(), source.replace("orders.created", "orders.updated"));
    assert_eq!(reopened.data.rabbitmq.unwrap().extra["futureSetting"], "preserved");
}

#[test]
fn rabbitmq_uses_the_selected_scope_and_encodes_credentials_and_virtual_hosts() {
    let config = RabbitMq {
        url: "{{broker}}".into(),
        username: "user@tenant".into(),
        password: "{{process.env.KESTREL_TEST_RABBITMQ_PASSWORD}}".into(),
        vhost: "/team staging".into(),
        ..Default::default()
    };
    let mut scope = Scope::default();
    scope.environment.insert("broker".into(), "amqps://rabbit.example:5671".into());
    scope.process_env.insert("KESTREL_TEST_RABBITMQ_PASSWORD".into(), "a:b@c/#".into());
    let resolved = config.resolve(&scope).unwrap();
    let uri = url::Url::parse(&resolved.connection_uri().unwrap()).unwrap();
    assert_eq!(uri.scheme(), "amqps");
    assert_eq!(uri.host_str(), Some("rabbit.example"));
    assert_eq!(uri.username(), "user%40tenant");
    assert_eq!(uri.password(), Some("a%3Ab%40c%2F%23"));
    assert_eq!(uri.path(), "/%2Fteam%20staging");
    assert_eq!(config.password, "{{process.env.KESTREL_TEST_RABBITMQ_PASSWORD}}");
}

#[test]
fn missing_password_variables_fail_without_exposing_other_resolved_values() {
    let config = RabbitMq { password: "{{missingRabbitPassword}}".into(), ..Default::default() };
    let error = config.resolve(&Scope::default()).unwrap_err().to_string();
    assert!(error.contains("missingRabbitPassword"));
    assert!(!error.contains("amqp://"));
}

#[test]
fn connecting_and_receiving_do_not_resolve_unused_publication_fields() {
    let config = RabbitMq {
        operation: Operation::Get,
        password: String::new(),
        queue: "{{queue}}".into(),
        body: "{{publicationPayload}}".into(),
        ..Default::default()
    };
    let mut scope = Scope::default();
    assert!(config.resolve_connection(&scope).is_ok());
    scope.environment.insert("queue".into(), "orders".into());
    assert_eq!(config.resolve(&scope).unwrap().queue, "orders");
    let publication = RabbitMq { operation: Operation::Publish, ..config };
    assert!(publication.resolve(&scope).is_err());
}

#[test]
fn broker_urls_and_saved_passwords_are_validated() {
    for url in ["http://localhost", "amqp://guest:secret@localhost", "amqp://localhost/another-vhost", "amqp://localhost?password=secret"] {
        let config = RabbitMq { url: url.into(), password: String::new(), ..Default::default() };
        assert!(config.connection_uri().is_err(), "{url}");
    }
    let mut config = RabbitMq::default();
    assert!(config.validate_saved_connection().is_ok());
    config.password = "literal-secret".into();
    assert!(config.validate_saved_connection().is_err());
    config.password = "{{password}}literal-secret{{password}}".into();
    assert!(config.validate_saved_connection().is_err());
    config.password = "{{password}}".into();
    config.url = "amqp://guest:secret@{{broker}}".into();
    assert!(config.validate_saved_connection().is_err());
}

#[test]
fn rabbitmq_operations_are_loaded_alongside_http_requests() {
    let root = std::env::temp_dir().join(format!("kestrel-rabbitmq-{}", uuid::Uuid::new_v4()));
    let mut collection = Collection::create(&root, "Mixed APIs").unwrap();
    let path = collection
        .add_request(
            &root,
            RequestFile {
                info: kestrel_core::format::Info { name: "Receive orders".into(), kind: "rabbitmq".into(), ..Default::default() },
                rabbitmq: Some(RabbitMq { operation: Operation::Get, queue: "orders".into(), ..Default::default() }),
                ..Default::default()
            },
        )
        .unwrap();
    collection.add_request(&root, RequestFile::new_http("Health", "GET", "https://example.test")).unwrap();
    assert_eq!(collection.requests().len(), 2);
    assert_eq!(collection.request(&path).unwrap().method(), "RMQ");
    collection.reload().unwrap();
    assert_eq!(collection.request(&path).unwrap().doc.data.rabbitmq.as_ref().unwrap().queue, "orders");
    std::fs::remove_dir_all(root).unwrap();
}
