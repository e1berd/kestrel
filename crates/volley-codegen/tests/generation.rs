use std::path::PathBuf;

use volley_codegen::{generate, generate_file};
use volley_core::format::{Auth, AuthConfig, Body, Defaults, KeyValue, RequestFile};
use volley_core::transfer::Progress;
use volley_core::{Collection, Document};

struct Fixture {
    directory: PathBuf,
    collection: Collection,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("kestrel-codegen-test-{}", uuid::Uuid::new_v4()));
        let collection = Collection::create(&directory.join("collection"), "Library").unwrap();
        Self { directory, collection }
    }

    fn add(&mut self, method: &str, url: &str) -> PathBuf {
        let root = self.collection.root.clone();
        self.collection.add_request(&root, RequestFile::new_http(&format!("{method} {url}"), method, url)).unwrap()
    }

    fn generated(&self) -> volley_codegen::Generated {
        generate(&self.collection, &Progress::default()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).expect("remove codegen fixture");
    }
}

#[test]
fn google_style_methods_follow_url_resources_and_encode_typed_path_arguments() {
    let mut fixture = Fixture::new();
    for (method, path) in [
        ("GET", "/users"),
        ("GET", "/users/:user_id"),
        ("POST", "/users"),
        ("PATCH", "/users/:user_id"),
        ("PUT", "/users/:user_id"),
        ("DELETE", "/users/:user_id"),
        ("GET", "/users/:user_id/messages"),
        ("GET", "/users/{user_id}/messages/{message_id}"),
        ("POST", "/users/{{user_id}}:archive"),
    ] {
        fixture.add(method, &format!("{{{{baseUrl}}}}/api/v1{path}"));
    }
    let generated = fixture.generated();
    assert_eq!(generated.requests, 9);
    assert_eq!(
        generated.methods,
        [
            "users.messages.get",
            "users.messages.list",
            "users.archive",
            "users.delete",
            "users.get",
            "users.insert",
            "users.list",
            "users.replace",
            "users.update"
        ]
    );
    assert!(generated.source.contains("\"userId\": string | number"));
    assert!(generated.source.contains("\"messageId\": string | number"));
    assert!(generated.source.contains("\"path\":\"/api/v1/users/{user_id}:archive\""));
    assert_eq!(generated.source, fixture.generated().source);
}

#[test]
fn duplicate_operations_require_explicit_names_and_never_lose_requests() {
    let mut fixture = Fixture::new();
    fixture.add("GET", "https://example.com/users");
    let path = fixture.add("GET", "https://example.com/users?active=true");
    let error = generate(&fixture.collection, &Progress::default()).unwrap_err();
    assert!(format!("{error:#}").contains("users.list"));
    let request = fixture.collection.request_mut(&path).unwrap();
    request.doc.data.extra.insert(
        "extensions".into(),
        serde_yaml_ng::from_str("kestrel:\n  codegen:\n    resource: users\n    method: listActive\n").unwrap(),
    );
    request.doc.save().unwrap();
    assert_eq!(fixture.generated().methods, ["users.list", "users.listActive"]);
}

#[test]
fn resource_and_method_conflicts_are_reported_in_either_order() {
    for urls in
        [["https://example.com/users", "https://example.com/users/list"], ["https://example.com/users/list", "https://example.com/users"]]
    {
        let mut fixture = Fixture::new();
        for url in urls {
            fixture.add("GET", url);
        }
        assert!(generate(&fixture.collection, &Progress::default()).is_err());
    }
}

#[test]
fn inherited_headers_and_auth_are_preserved_without_credentials_or_environment_values() {
    let mut fixture = Fixture::new();
    let mut auth = AuthConfig::new("bearer");
    auth.set("token", "literal-secret-token");
    fixture.collection.doc.data.request = Some(Defaults {
        auth: Some(Auth::Config(auth)),
        headers: vec![KeyValue::new("X-Scope", "collection"), KeyValue::new("Authorization", "literal-authorization")],
        variables: vec![KeyValue::new("token", "literal-variable-secret")],
        ..Default::default()
    });
    fixture.collection.doc.save().unwrap();
    let root = fixture.collection.root.clone();
    let folder = fixture.collection.add_folder(&root, "People").unwrap();
    let mut folder_document = Document::<volley_core::format::FolderFile>::read(&folder.join("folder.yml"), true).unwrap();
    folder_document.data.request = Some(Defaults { headers: vec![KeyValue::new("X-Scope", "folder")], ..Default::default() });
    folder_document.save().unwrap();
    fixture.collection.reload().unwrap();
    let mut request = RequestFile::new_http("Read user", "GET", "{{baseUrl}}/users/:user_id?access_token=literal-query-token");
    let http = request.http.as_mut().unwrap();
    http.headers.push(KeyValue::new("Cookie", "literal-cookie"));
    http.headers.push(KeyValue::new("X-Request", "request"));
    let mut private_header = KeyValue::new("X-Private", "literal-private-header");
    private_header.extra.insert("secret".into(), true.into());
    http.headers.push(private_header);
    let mut private_query = KeyValue::new("opaque", "literal-private-query");
    private_query.extra.insert("secret".into(), true.into());
    http.params.push(private_query);
    fixture.collection.add_request(&folder, request).unwrap();
    std::fs::write(root.join(".env"), "DOTENV_SECRET=literal-dotenv-secret\n").unwrap();
    fixture.collection.reload().unwrap();
    let generated = fixture.generated();
    assert!(generated.source.contains("\"x-scope\":\"folder\""));
    assert!(generated.source.contains("\"x-request\":\"request\""));
    assert!(generated.source.contains("{{headers.cookie}}"));
    assert!(generated.source.contains("{{query.access_token}}"));
    assert!(generated.source.contains("{{headers.x-private}}"));
    assert!(generated.source.contains("{{query.opaque}}"));
    assert!(!generated.source.contains("literal-"));
}

#[test]
fn query_parameters_bodies_and_disabled_fields_are_emitted() {
    let mut fixture = Fixture::new();
    let path = fixture.add("POST", "{{host}}/users?tag=one&tag=two");
    let request = fixture.collection.request_mut(&path).unwrap();
    let http = request.doc.data.http.as_mut().unwrap();
    http.body = Some(Body::text("json", "{\"password\": \"never-embed-body-examples\"}"));
    http.params.push(KeyValue::new("page_size", "10"));
    http.params.push(KeyValue { disabled: true, ..KeyValue::new("disabled_query", "not-emitted") });
    request.doc.save().unwrap();
    let source = fixture.generated().source;
    assert!(source.contains("body: JsonValue"));
    assert!(source.contains("\"page_size\"?: QueryValue"));
    assert!(!source.contains("disabled_query"));
    assert!(!source.contains("never-embed-body-examples"));
    assert!(source.contains("[\"tag\",\"one\"],[\"tag\",\"two\"]"));
}

#[test]
fn paths_keep_trailing_and_repeated_slashes() {
    let mut fixture = Fixture::new();
    fixture.add("GET", "https://example.com//users/:user_id/");
    let generated = fixture.generated();
    assert_eq!(generated.methods, ["users.get"]);
    assert!(generated.source.contains("\"path\":\"//users/{user_id}/\""));
}

#[test]
fn invalid_and_unsupported_requests_fail_before_publishing() {
    for url in [
        "https://username:password@example.com/users",
        "ftp://example.com/users",
        "{{baseUrl}}users",
        "/users/:body",
        "/users/:query",
        "/users/:user_id/:user-id",
    ] {
        let mut fixture = Fixture::new();
        fixture.add("GET", url);
        assert!(generate(&fixture.collection, &Progress::default()).is_err(), "{url}");
    }
    let mut fixture = Fixture::new();
    let path = fixture.add("GET", "/users");
    let request = fixture.collection.request_mut(&path).unwrap();
    request.doc.data.http.as_mut().unwrap().body = Some(Body::text("json", "{}"));
    assert!(generate(&fixture.collection, &Progress::default()).is_err());
}

#[test]
fn writing_is_atomic_respects_cancellation_and_preserves_collection_bytes() {
    let mut fixture = Fixture::new();
    let path = fixture.add("GET", "/users/:user_id");
    let original = std::fs::read(&path).unwrap();
    let destination = fixture.directory.join("client.ts");
    let progress = Progress::default();
    let generated = generate_file(&fixture.collection.root, &destination, false, &progress).unwrap();
    assert!(progress.snapshot().finished);
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), generated.source);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(generate_file(&fixture.collection.root, &destination, false, &Progress::default()).is_err());
    assert!(generate_file(&fixture.collection.root, &fixture.directory.join("request.yml"), true, &Progress::default()).is_err());
    let cancelled = Progress::default();
    cancelled.cancel();
    assert!(generate_file(&fixture.collection.root, &destination, true, &cancelled).is_err());
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), generated.source);
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 2);
}
