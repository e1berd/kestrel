use serde_json::Value as Json;
use volley_core::Collection;

use super::support::server::Server;
use super::support::{Directory, assert_save_preserves_bytes, fixtures, snapshot, succeed};

#[test]
fn imported_requests_execute_against_a_local_http_server() {
    let directory = Directory::new();
    for (index, (source, method, expected_path)) in [
        ("upstream/oai/v2.0/json/petstore-simple.json", "POST", "/pets"),
        ("upstream/oai/v3.0/petstore-expanded.yaml", "POST", "/pets"),
        ("upstream/oai/v3.1/tictactoe.json", "PUT", "/board/1/1"),
        ("upstream/oai/v3.2/3.2-query-example.json", "QUERY", "/flights/search"),
        ("apidog.json", "GET", "/pets/42"),
    ]
    .into_iter()
    .enumerate()
    {
        let server = Server::new(|_| (200, br#"{"ok":true}"#.to_vec()));
        let source = fixtures().join(source);
        let destination = directory.0.join(index.to_string());
        succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&destination));
        let collection = Collection::open(&destination).unwrap();
        let requests = collection.requests();
        let request = requests.iter().find(|request| request.method() == method).unwrap();
        let output = succeed(
            directory
                .cli()
                .arg("run")
                .arg(&request.doc.path)
                .arg("--json")
                .args(["--var", &format!("baseUrl={}", server.url)])
                .env("API_TOKEN", "local-test-token"),
        );
        let outcome: Json = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(outcome["status"], 200, "{}", source.display());
        assert!(outcome["error"].is_null());
        let received = server.requests();
        assert_eq!(received.len(), 1);
        let received = &received[0];
        assert_eq!(received.method, method);
        assert_eq!(received.path, expected_path);
        match method {
            "GET" => {
                assert_eq!(received.headers["authorization"], "Bearer local-test-token");
                assert_eq!(received.headers["x-client"], "Kestrel");
                assert!(received.body.is_empty());
            }
            "QUERY" => {
                let body: Json = serde_json::from_slice(&received.body).unwrap();
                assert_eq!(body["origin"], "ATL");
                assert_eq!(body["destination"], "LHR");
                assert_eq!(body["passengers"], 2);
            }
            "PUT" => assert_eq!(serde_json::from_slice::<Json>(&received.body).unwrap(), "."),
            _ => {
                let body: Json = serde_json::from_slice(&received.body).unwrap();
                assert!(body.get("name").is_some(), "lost the imported request body: {body}");
            }
        }
    }
}

#[test]
fn imports_and_syncs_public_multifile_specs_over_http() {
    let directory = Directory::new();
    let root = fixtures().join("upstream/oai/v2.0/json/petstore-separate");
    let server = Server::new(move |request| {
        let path = root.join(request.path.trim_start_matches('/'));
        match std::fs::read(path) {
            Ok(body) => (200, body),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (404, b"missing fixture".to_vec()),
            Err(error) => panic!("serve fixture: {error}"),
        }
    });
    let source = format!("{}/spec/swagger.json", server.url);
    let destination = directory.0.join("remote");
    succeed(directory.cli().args(["import", &source]).arg("--out").arg(&destination));
    let collection = Collection::open(&destination).unwrap();
    assert_eq!(collection.requests().len(), 4);
    assert_eq!(collection.doc.data.openapi_sources()[0].0, source);
    assert_save_preserves_bytes(&destination);
    let before = snapshot(&destination);
    succeed(directory.cli().arg("sync").arg(&destination));
    assert_eq!(snapshot(&destination), before);
    let received = server.requests();
    for path in ["/spec/swagger.json", "/spec/parameters.json", "/spec/Pet.json", "/spec/NewPet.json", "/common/Error.json"] {
        assert!(received.iter().any(|request| request.path == path), "missing HTTP reference request {path}: {received:?}");
    }
}

#[test]
fn failed_http_import_leaves_no_collection_or_temporary_files() {
    let directory = Directory::new();
    let server = Server::new(|_| (404, b"missing fixture".to_vec()));
    let destination = directory.0.join("missing");
    let output = directory.cli().args(["import", &format!("{}/missing.json", server.url)]).arg("--out").arg(&destination).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("404"));
    assert!(!destination.exists());
    assert!(!snapshot(&directory.0).keys().any(|path| path.to_string_lossy().contains(".kestrel-import-")));
}
