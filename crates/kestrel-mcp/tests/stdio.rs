use kestrel_core::Collection;
use serde_json::json;

mod support;
use support::{Fixture, call, value};

#[tokio::test]
async fn discovers_tools_and_edits_without_losing_bruno_fields() {
    let fixture = Fixture::new();
    let path = fixture.root.join("request.yml");
    let original = "info:\n  name: Original\n  type: http\n  seq: 1\n  vendor: retained\n\nhttp:\n  method: GET\n  url: https://example.invalid\n  vendor: retained\n\nvendor:\n  enabled: true\n";
    std::fs::write(&path, original).unwrap();
    let client = fixture.connect().await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 9);
    let read = tools.iter().find(|tool| tool.name == "get_request").unwrap();
    assert_eq!(read.annotations.as_ref().unwrap().read_only_hint, Some(true));
    assert_eq!(value(&client, "collection_info", json!({})).await["name"], "MCP test");
    let requests = value(&client, "list_requests", json!({})).await;
    assert_eq!(requests["requests"][0]["path"], "request.yml");
    let request = value(&client, "get_request", json!({"path": "request.yml"})).await;
    assert_eq!(request["request"]["http"]["vendor"], "retained");
    value(&client, "update_request", json!({"path": "request.yml", "patch": {}})).await;
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    value(&client, "update_request", json!({"path": "request.yml", "patch": {"http": {"url": "https://example.invalid/users"}}})).await;
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original.replace("https://example.invalid", "https://example.invalid/users"));
    let created = value(&client, "create_request", json!({"name": "New request", "method": "post", "url": "{{baseUrl}}/users"})).await;
    let collection = Collection::open(&fixture.root).unwrap();
    assert_eq!(collection.requests().len(), 2);
    assert_eq!(created["request"]["http"]["method"], "POST");
    assert_eq!(call(&client, "get_request", json!({"path": "../opencollection.yml"})).await.is_error, Some(true));
    assert_eq!(call(&client, "get_request", json!({"path": "opencollection.yml"})).await.is_error, Some(true));
    assert_eq!(call(&client, "update_request", json!({"path": "request.yml", "patch": {"http": null}})).await.is_error, Some(true));
    assert_eq!(
        call(&client, "create_request", json!({"name": "Bad", "method": "GET\r\nOops", "url": "https://example.invalid"})).await.is_error,
        Some(true)
    );
    assert_eq!(
        call(&client, "create_request", json!({"name": "folder", "method": "GET", "url": "https://example.invalid"})).await.is_error,
        Some(true)
    );
    assert!(!fixture.root.join("folder.yml").exists());
    let log = std::fs::read_to_string(fixture.root.join("state/kestrel/errors.log")).unwrap();
    assert!(log.contains("execute an MCP tool"));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn hides_secret_environment_values() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.root.join("environments")).unwrap();
    std::fs::write(fixture.root.join("environments/local.yml"), "name: Local\nvariables:\n  - name: baseUrl\n    value: https://example.invalid\n  - name: token\n    value: hidden-secret\n    secret: true\n").unwrap();
    let client = fixture.connect().await;
    let environments = value(&client, "list_environments", json!({})).await;
    assert_eq!(environments["environments"][0]["variables"][0]["value"], "https://example.invalid");
    assert!(environments["environments"][0]["variables"][1]["value"].is_null());
    assert!(!environments.to_string().contains("hidden-secret"));
    client.cancel().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_requests_reached_through_external_symlinks() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.root.join("outside.yml"), "info:\n  name: Outside\nhttp:\n  method: GET\n  url: https://example.invalid\n")
        .unwrap();
    std::os::unix::fs::symlink(outside.root.join("outside.yml"), fixture.root.join("linked.yml")).unwrap();
    let client = fixture.connect().await;
    assert_eq!(call(&client, "get_request", json!({"path": "linked.yml"})).await.is_error, Some(true));
    assert_eq!(call(&client, "list_requests", json!({})).await.is_error, Some(true));
    assert_eq!(call(&client, "send_request", json!({"path": "linked.yml"})).await.is_error, Some(true));
    client.cancel().await.unwrap();
}
