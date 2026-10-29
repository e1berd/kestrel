use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use volley_core::{Collection, Document};
use volley_import::{Format, PreparedImport};
use volley_openapi::{GroupBy, SyncOptions};

const APIDOG: &str = include_str!("fixtures/apidog.json");

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kestrel-import-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove test directory");
    }
}

fn prepare(text: &str) -> PreparedImport {
    PreparedImport::parse(text, "/tmp/spec.json", None, GroupBy::Tags).unwrap()
}

#[test]
fn imports_native_apidog_folders_requests_schemas_and_warnings() {
    let directory = Directory::new();
    let prepared = prepare(APIDOG);
    assert_eq!(prepared.preview.format, Format::Apidog);
    assert_eq!((prepared.preview.requests, prepared.preview.folders), (4, 2));
    assert!(prepared.preview.warnings.iter().any(|warning| warning.contains("Environments")));
    assert!(prepared.preview.warnings.iter().any(|warning| warning.contains("processors")));
    let collection = prepared.save(&directory.0.join("pets"), None).unwrap();
    assert_eq!(collection.name(), "Pet store");
    assert!(collection.doc.data.openapi_sources().is_empty());
    let get = collection.requests().into_iter().find(|request| request.name() == "Get pet").unwrap();
    assert!(get.doc.path.ends_with("Pets/Details/Get pet.yml"));
    assert_eq!(get.doc.data.http().url, "{{baseUrl}}/pets/:id");
    let http = get.doc.data.http();
    assert_eq!(http.params[0].value, "42");
    assert!(http.params.iter().any(|parameter| parameter.name == "full" && parameter.disabled));
    assert_eq!(http.headers[0].value, "Kestrel");
    assert_eq!(http.auth.unwrap().config().unwrap().str("token"), "{{process.env.API_TOKEN}}");
    let requests = collection.requests();
    let create: Vec<_> = requests.iter().filter(|request| request.name() == "Create pet").collect();
    assert_eq!(create.len(), 2);
    assert_ne!(create[0].doc.path, create[1].doc.path);
    assert!(create.iter().any(|request| request.doc.data.http().body.unwrap().as_text().contains("Milo")));
    assert!(create.iter().any(|request| request.doc.data.http().body.unwrap().as_text().contains("Rex")));
    let upload = requests.iter().find(|request| request.name() == "Upload").unwrap();
    let fields = upload.doc.data.http().body.unwrap().fields();
    assert_eq!(fields[0].kind.as_deref(), Some("file"));
    assert!(fields[1].disabled);
    for request in requests {
        let original = std::fs::read(&request.doc.path).unwrap();
        let mut document = Document::<volley_core::format::RequestFile>::read(&request.doc.path, true).unwrap();
        document.save().unwrap();
        assert_eq!(std::fs::read(&request.doc.path).unwrap(), original);
        let text = String::from_utf8(original).unwrap();
        assert!(!text.contains("must-not-be-written"));
    }
}

#[test]
fn imports_flat_http_collection_with_children() {
    let directory = Directory::new();
    let document = json!({"apidogProject": "1.1.0", "name": "Flat export", "httpCollection": [{
        "name": "Root", "items": [], "children": [{"name": "Nested", "items": [
            {"name": "Health", "method": "get", "path": "https://api.example.com/health"}
        ], "children": []}]
    }]});
    let prepared = prepare(&document.to_string());
    let collection = prepared.save(&directory.0.join("flat"), Some("Renamed")).unwrap();
    assert_eq!(collection.name(), "Renamed");
    assert_eq!(collection.requests()[0].doc.data.http().url, "https://api.example.com/health");
    assert!(collection.requests()[0].doc.path.ends_with("Root/Nested/Health.yml"));
}

#[test]
fn openapi_versions_and_swagger_keep_sync_source_and_user_edits() {
    let directory = Directory::new();
    for version in ["3.0.4", "3.1.2", "3.2.1", "2.0"] {
        let mut document = json!({"info": {"title": "Pets"}, "paths": {"/pets": {"get": {"summary": "List pets", "tags": ["Pets"]}}}});
        document[if version == "2.0" { "swagger" } else { "openapi" }] = json!(version);
        let source = directory.0.join(format!("{version}.json"));
        std::fs::write(&source, document.to_string()).unwrap();
        let prepared = PreparedImport::load(&source.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
        let mut collection = prepared.save(&directory.0.join(version), None).unwrap();
        let (remembered, _) = collection.doc.data.openapi_sources().remove(0);
        assert!(collection.root.join(&remembered).is_file());
        let path = collection.requests()[0].doc.path.clone();
        let request = collection.request_mut(&path).unwrap();
        request.doc.data.http.as_mut().unwrap().headers.push(volley_core::format::KeyValue::new("X-Custom", "kept"));
        request.doc.save().unwrap();
        let before = std::fs::read(&path).unwrap();
        let spec = volley_openapi::load(&remembered, &collection.root).unwrap();
        let report =
            volley_openapi::sync(&mut collection, &spec, &SyncOptions { group_by: GroupBy::Tags, prune: false, dry_run: false }).unwrap();
        assert_eq!((report.updated.len(), report.added.len(), report.unchanged), (0, 0, 1));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

#[test]
fn rejects_invalid_files_and_explicit_format_mismatch() {
    for input in [
        "{}",
        "[]",
        "not a spec",
        "{broken",
        r#"{"openapi":"30.0.0"}"#,
        r#"{"apidogProject":"2.0.0","apiCollection":[]}"#,
        r#"{"apidogProject":"1.0.0","apiCollection":{}}"#,
        r#"{"openapi":"3.2.1","info":{},"paths":[]}"#,
        r#"{"openapi":"3.2.1","info":{},"paths":{}}"#,
    ] {
        assert!(PreparedImport::parse(input, "input.json", None, GroupBy::Tags).is_err(), "{input}");
    }
    assert!(PreparedImport::parse(APIDOG, "input.json", Some(Format::OpenApi), GroupBy::Tags).is_err());
}

#[test]
fn never_modifies_an_existing_destination() {
    let directory = Directory::new();
    let destination = directory.0.join("existing");
    let collection = Collection::create(&destination, "Existing").unwrap();
    let before = std::fs::read(&collection.doc.path).unwrap();
    assert!(prepare(APIDOG).save(&destination, None).is_err());
    assert_eq!(std::fs::read(&collection.doc.path).unwrap(), before);
    assert!(Collection::open(&destination).unwrap().requests().is_empty());
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn imports_yaml_and_utf8_bom() {
    let yaml = "\u{feff}openapi: 3.1.2\ninfo:\n  title: YAML API\npaths:\n  /health:\n    get:\n      summary: Health\n";
    assert_eq!(prepare(yaml).preview.requests, 1);
}

#[test]
fn sanitizes_folders_without_merging_name_collisions() {
    let directory = Directory::new();
    let mut document: Json = serde_json::from_str(APIDOG).unwrap();
    document["apiCollection"] = json!([
        {"name": "../outside", "items": [{"name": "Health", "api": {"method": "get", "path": "/a"}}]},
        {"name": "..\\outside", "items": [{"name": "Health", "api": {"method": "get", "path": "/b"}}]}
    ]);
    let collection = prepare(&document.to_string()).save(&directory.0.join("safe"), None).unwrap();
    assert_eq!(collection.requests().len(), 2);
    assert!(collection.requests().iter().all(|request| request.doc.path.starts_with(&collection.root)));
    assert_ne!(collection.requests()[0].doc.path, collection.requests()[1].doc.path);
}

#[test]
fn failed_writes_leave_the_destination_empty_and_remove_staging_files() {
    let directory = Directory::new();
    let destination = directory.0.join("empty");
    std::fs::create_dir(&destination).unwrap();
    let mut document: Json = serde_json::from_str(APIDOG).unwrap();
    document["apiCollection"][0]["items"][0]["items"][0]["name"] = json!("x".repeat(300));
    let prepared = prepare(&document.to_string());
    assert!(prepared.save(&destination, None).is_err());
    assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn imports_custom_methods_graphql_response_examples_and_redacts_credentials() {
    let directory = Directory::new();
    let document = json!({"apidogProject": "1.1.0", "httpCollection": [{"name": "API", "items": [{
        "name": "GraphQL", "method": "propfind", "path": "/graphql",
        "auth": {"type": "apikey", "apikey": {"key": "api_key", "in": "query", "value": "not-for-git"}},
        "parameters": {"header": [{"name": "Authorization", "example": "Bearer not-for-git"}]},
        "requestBody": {"type": "graphql", "graphql": {"query": "query { pets { id } }", "variables": "{}"}},
        "responses": [{"id": "ok", "code": 200, "contentType": "json", "jsonSchema": {"type": "object"}}],
        "responseExamples": [{"name": "Success", "responseId": "ok", "data": "{\"pets\":[]}"}]
    }]}]});
    let collection = prepare(&document.to_string()).save(&directory.0.join("graphql"), None).unwrap();
    let request = collection.requests()[0];
    let http = request.doc.data.http();
    assert_eq!(http.method, "PROPFIND");
    assert_eq!(http.body.unwrap().graphql(), ("query { pets { id } }".into(), "{}".into()));
    assert_eq!(http.auth.unwrap().config().unwrap().str("placement"), "query");
    let examples = request.doc.data.extra["examples"].as_sequence().unwrap();
    assert_eq!(examples[0]["response"]["headers"][0]["value"].as_str(), Some("application/json"));
    assert!(examples[0]["response"]["body"]["data"].as_str().unwrap().contains("pets"));
    let file = std::fs::read_to_string(&request.doc.path).unwrap();
    assert!(!file.contains("not-for-git"));
    assert!(file.contains("process.env.AUTHORIZATION"));
}
