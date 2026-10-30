use std::path::PathBuf;
use std::process::Command;

use kestrel_codegen::DataLayer;
use kestrel_core::Collection;
use kestrel_core::format::{Auth, AuthConfig, Body, Defaults, KeyValue, RequestFile};
use kestrel_core::transfer::Progress;

struct Directory(PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove SDK runtime fixture");
    }
}

#[test]
#[ignore = "requires Node.js 24 or newer; set KESTREL_TYPESCRIPT_COMPILER to also check declarations"]
fn generated_sdk_sends_requests_and_obeys_its_types() {
    let directory = Directory(std::env::temp_dir().join(format!("kestrel-sdk-runtime-{}", uuid::Uuid::new_v4())));
    let mut collection = Collection::create(&directory.0.join("collection"), "Library").unwrap();
    let mut auth = AuthConfig::new("bearer");
    auth.set("token", "{{process.env.API_TOKEN}}");
    collection.doc.data.request = Some(Defaults { auth: Some(Auth::Config(auth)), ..Default::default() });
    collection.doc.save().unwrap();
    let root = collection.root.clone();
    for (method, path, body) in [
        ("GET", "/users?tag=one&tag=two&limit=10", None),
        ("GET", "/users/:user_id", None),
        ("QUERY", "/users?limit=10", Some("json")),
        ("QUERY", "/users/:user_id/messages/:message_id", Some("json")),
        ("POST", "/users", Some("json")),
        ("PATCH", "/users/:user_id", Some("json")),
        ("DELETE", "/users/:user_id", None),
        ("GET", "/users/:user_id/messages/:message_id", None),
        ("GET", "/users/:user_id/messages", None),
        ("POST", "/users/:user_id:archive", None),
        ("POST", "/forms", Some("form-urlencoded")),
        ("POST", "/uploads", Some("multipart-form")),
        ("POST", "/graphs", Some("graphql")),
        ("POST", "/xml", Some("xml")),
        ("HEAD", "/health", None),
        ("GET", "/usersMessages", None),
    ] {
        let mut request = RequestFile::new_http(&format!("{method} {path}"), method, &format!("{{{{baseUrl}}}}/v1{path}"));
        let http = request.http.as_mut().unwrap();
        http.headers = vec![KeyValue::new("X-Scope", "{{scope}}"), KeyValue::new("X-Default", "collection")];
        http.body = body.map(|kind| Body::text(kind, ""));
        collection.add_request(&root, request).unwrap();
    }
    for (method, path, metadata) in [
        ("GET", "/profile", "kestrel:\n  codegen:\n    method: get\n    collection: false\n"),
        (
            "QUERY",
            "/search",
            "kestrel:\n  codegen:\n    collection:\n      items: payload.results\n      nextPageToken: meta.next\n      pageToken: cursor\n",
        ),
    ] {
        let mut request = RequestFile::new_http(path, method, &format!("{{{{baseUrl}}}}/v1{path}"));
        request.extra.insert("extensions".into(), serde_yaml_ng::from_str(metadata).unwrap());
        if method == "QUERY" {
            request.http.as_mut().unwrap().body = Some(Body::text("json", ""));
        }
        collection.add_request(&root, request).unwrap();
    }
    for (path, kind, placement) in
        [("/keys", "apikey", "query"), ("/headerKeys", "apikey", "header"), ("/basic", "basic", "header"), ("/oauth", "oauth2", "header")]
    {
        let mut request = RequestFile::new_http(path, "GET", &format!("https://example.com{path}"));
        let mut auth = AuthConfig::new(kind);
        auth.set("key", "X-Key");
        auth.set("placement", placement);
        request.http.as_mut().unwrap().auth = Some(Auth::Config(auth));
        collection.add_request(&root, request).unwrap();
    }
    for (method, url, service) in [
        ("GET", "https://billing.example.com/invoices/:invoice_id", "billing"),
        ("GET", "https://files.example.com/files/:file_id", "files"),
        ("POST", "https://session.example.com/sessions", "session"),
    ] {
        let mut request = RequestFile::new_http(service, method, url);
        request
            .extra
            .insert("extensions".into(), serde_yaml_ng::from_str(&format!("kestrel:\n  codegen:\n    service: {service}\n")).unwrap());
        let http = request.http.as_mut().unwrap();
        http.auth = Some(if service == "billing" {
            let mut auth = AuthConfig::new("bearer");
            auth.set("token", "{{process.env.API_TOKEN}}");
            Auth::Config(auth)
        } else {
            Auth::Mode("none".into())
        });
        if method == "POST" {
            http.body = Some(Body::text("json", ""));
        }
        collection.add_request(&root, request).unwrap();
    }
    let mut microservices = Collection::create(&directory.0.join("microservices"), "Microservices").unwrap();
    let microservices_root = microservices.root.clone();
    for service in ["alpha", "beta"] {
        let mut request = RequestFile::new_http(service, "GET", &format!("https://{service}.example.com/users/:user_id"));
        request.extra.insert(
            "extensions".into(),
            serde_yaml_ng::from_str(&format!("kestrel:\n  codegen:\n    service: {service}\n    resource: {service}.users\n")).unwrap(),
        );
        request.http.as_mut().unwrap().auth = Some(Auth::Config(AuthConfig::new("bearer")));
        microservices.add_request(&microservices_root, request).unwrap();
    }
    let vue_package = std::env::var_os("KESTREL_VUE").map(PathBuf::from);
    let rstore_modules = std::env::var_os("KESTREL_RSTORE_MODULES").map(PathBuf::from);
    let adapters = [(DataLayer::None, "plain"), (DataLayer::TanstackQueryReact, "react")]
        .into_iter()
        .chain(vue_package.is_some().then_some((DataLayer::TanstackQueryVue, "vue")))
        .chain(rstore_modules.is_some().then_some((DataLayer::Rstore, "rstore")));
    for (data_layer, label) in adapters {
        let directory = directory.0.join(label);
        std::fs::create_dir(&directory).unwrap();
        if let Some(package) = vue_package.as_ref().filter(|_| data_layer == DataLayer::TanstackQueryVue) {
            std::fs::create_dir(directory.join("node_modules")).unwrap();
            std::os::unix::fs::symlink(package, directory.join("node_modules/vue")).unwrap();
        }
        if let Some(modules) = rstore_modules.as_ref().filter(|_| data_layer == DataLayer::Rstore) {
            std::os::unix::fs::symlink(modules, directory.join("node_modules")).unwrap();
        }
        let single_file = kestrel_codegen::OutputOptions { single_file: true, data_layer, ..Default::default() };
        let other_destination = directory.join("microservices.js");
        kestrel_codegen::generate_to(&microservices_root, &other_destination, single_file, false, &Progress::default()).unwrap();
        let destination = directory.join("client.js");
        std::fs::write(directory.join("package.json"), "{\"type\":\"module\"}").unwrap();
        kestrel_codegen::generate_to(&root, &destination, single_file, false, &Progress::default()).unwrap();
        if let Some(compiler) = std::env::var_os("KESTREL_TYPESCRIPT_COMPILER") {
            let valid = directory.join("valid.ts");
            std::fs::write(&valid, runtime_output::valid_source(data_layer)).unwrap();
            let output = Command::new("node")
                .arg(&compiler)
                .args([
                    "--strict",
                    "--noEmit",
                    "--target",
                    "ES2022",
                    "--lib",
                    "ES2022,ESNext.Array,DOM",
                    "--module",
                    "NodeNext",
                    "--allowImportingTsExtensions",
                ])
                .args(runtime_output::skip_library_check(label))
                .arg(&valid)
                .output()
                .expect("typecheck generated SDK");
            assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            std::fs::write(
                &valid,
                "import { createClient } from './client.js'; createClient().users.get({}); createClient().users.list().then(() => {});",
            )
            .unwrap();
            let output = Command::new("node")
                .arg(compiler)
                .args([
                    "--strict",
                    "--noEmit",
                    "--target",
                    "ES2022",
                    "--lib",
                    "ES2022,ESNext.Array,DOM",
                    "--module",
                    "NodeNext",
                    "--allowImportingTsExtensions",
                ])
                .arg(&valid)
                .output()
                .expect("check that missing path arguments are rejected");
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("userId"));
            assert!(String::from_utf8_lossy(&output.stdout).contains("then"));
        }
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/runtime.mjs");
        let output = Command::new("node")
            .env("KESTREL_QUERY_ADAPTER", label)
            .arg(script)
            .arg(&destination)
            .arg(&other_destination)
            .output()
            .expect("run Node.js 24");
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        if let Some(compiler) = std::env::var_os("KESTREL_TYPESCRIPT_COMPILER") {
            runtime_output::verify(&directory, &root, &microservices_root, &compiler, data_layer, label);
        }
    }
}
mod runtime_output;
