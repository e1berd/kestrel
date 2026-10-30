use kestrel_core::Collection;
use kestrel_core::format::{RequestFile, Runtime, Script};
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

mod support;
use support::{Fixture, call, value};

#[tokio::test]
async fn sends_with_inheritance_scripts_tests_and_persistent_session() {
    let fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 8192];
            let bytes = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]);
            assert!(request.starts_with("GET /users HTTP/1.1"));
            assert!(request.to_lowercase().contains("x-inherited: yes"));
            if index == 1 {
                assert!(request.to_lowercase().contains("cookie: session=retained"));
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nContent-Type: application/json\r\nSet-Cookie: session=retained; Path=/\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
        }
    });
    let mut collection = Collection::open(&fixture.root).unwrap();
    collection.doc.data.request = Some(kestrel_core::format::Defaults {
        headers: vec![kestrel_core::format::KeyValue::new("X-Inherited", "yes")],
        ..Default::default()
    });
    collection.doc.save().unwrap();
    let mut request = RequestFile::new_http("Users", "GET", &format!("http://{address}/{{{{route}}}}"));
    request.runtime = Some(Runtime {
        scripts: vec![Script {
            kind: "after-response".into(),
            code: "test('status', () => expect(res.getStatus()).to.equal(200)); bru.setVar('saved', 'yes'); console.log('finished');"
                .into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    let path = collection.add_request(&fixture.root, request).unwrap();
    let relative = path.strip_prefix(&fixture.root).unwrap().to_str().unwrap();
    let client = fixture.connect().await;
    let result = value(&client, "send_request", json!({"path": relative, "variables": {"route": "users"}})).await;
    assert_eq!(result["passed"], true);
    assert_eq!(result["results"][0]["response"]["body"], "{\"ok\":true}");
    assert_eq!(result["results"][0]["tests"][0]["passed"], true);
    assert_eq!(result["results"][0]["logs"][0]["text"], "finished");
    let result = value(&client, "run_collection", json!({})).await;
    assert_eq!(result["passed"], true);
    assert_eq!(result["executed"], 1);
    assert_eq!(call(&client, "send_request", json!({"path": relative, "environment": "Missing"})).await.is_error, Some(true));
    assert_eq!(call(&client, "send_request", json!({"path": relative, "timeout_seconds": 0})).await.is_error, Some(true));
    assert_eq!(call(&client, "send_request", json!({})).await.is_error, Some(true));
    assert_eq!(value(&client, "reset_session", json!({})).await["reset"], true);
    server.await.unwrap();
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn reports_network_failures_in_results_and_error_log() {
    let fixture = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut collection = Collection::open(&fixture.root).unwrap();
    let request = RequestFile::new_http("Unavailable", "GET", &format!("http://{address}"));
    let path = collection.add_request(&fixture.root, request).unwrap();
    let client = fixture.connect().await;
    let result = value(&client, "send_request", json!({"path": path.file_name().unwrap().to_str().unwrap()})).await;
    assert_eq!(result["passed"], false);
    assert!(result["results"][0]["error"].as_str().is_some());
    assert!(std::fs::read_to_string(fixture.root.join("state/kestrel/errors.log")).unwrap().contains("execute an MCP request"));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn limits_script_redirects_and_truncates_large_bodies() {
    let fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            assert!(stream.read(&mut buffer).await.unwrap() > 0);
            let body = "x".repeat(70_000);
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let mut collection = Collection::open(&fixture.root).unwrap();
    let mut request = RequestFile::new_http("Loop", "GET", &format!("http://{address}"));
    request.runtime = Some(Runtime {
        scripts: vec![Script { kind: "after-response".into(), code: "bru.setNextRequest('Loop');".into(), ..Default::default() }],
        ..Default::default()
    });
    collection.add_request(&fixture.root, request).unwrap();
    let client = fixture.connect().await;
    let result = value(&client, "run_collection", json!({"max_requests": 2})).await;
    assert_eq!(result["passed"], false);
    assert_eq!(result["executed"], 2);
    assert!(result["error"].as_str().unwrap().contains("max_requests"));
    let response = &result["results"][0]["response"];
    assert_eq!(response["body_truncated"], true);
    assert_eq!(response["body_bytes"], 70_000);
    assert_eq!(response["body"].as_str().unwrap().len(), 65_536);
    server.await.unwrap();
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn request_timeout_keeps_the_server_available() {
    let fixture = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 8192];
        assert!(stream.read(&mut buffer).await.unwrap() > 0);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    });
    let mut collection = Collection::open(&fixture.root).unwrap();
    let request = RequestFile::new_http("Slow", "GET", &format!("http://{address}"));
    let path = collection.add_request(&fixture.root, request).unwrap();
    let client = fixture.connect().await;
    let result = value(&client, "send_request", json!({"path": path.file_name().unwrap().to_str().unwrap(), "timeout_seconds": 1})).await;
    assert_eq!(result["passed"], false);
    assert!(result["results"][0]["error"].as_str().unwrap().contains("timed out"));
    assert_eq!(value(&client, "collection_info", json!({})).await["requests"], 1);
    server.await.unwrap();
    client.cancel().await.unwrap();
}
