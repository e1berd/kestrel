use std::path::PathBuf;
use std::process::Command;

use volley_core::Collection;
use volley_core::format::{Auth, AuthConfig, Body, Defaults, KeyValue, RequestFile};
use volley_core::transfer::Progress;

struct Directory(PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove SDK runtime fixture");
    }
}

#[test]
#[ignore = "requires Node.js 24 or newer; set KESTREL_TYPESCRIPT_COMPILER to also run tsc"]
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
        ("POST", "/users", Some("json")),
        ("PATCH", "/users/:user_id", Some("json")),
        ("DELETE", "/users/:user_id", None),
        ("GET", "/users/:user_id/messages/:message_id", None),
        ("POST", "/users/:user_id:archive", None),
        ("POST", "/forms", Some("form-urlencoded")),
        ("POST", "/uploads", Some("multipart-form")),
        ("POST", "/graphs", Some("graphql")),
        ("POST", "/xml", Some("xml")),
        ("HEAD", "/health", None),
    ] {
        let mut request = RequestFile::new_http(&format!("{method} {path}"), method, &format!("{{{{baseUrl}}}}/v1{path}"));
        let http = request.http.as_mut().unwrap();
        http.headers = vec![KeyValue::new("X-Scope", "{{scope}}"), KeyValue::new("X-Default", "collection")];
        http.body = body.map(|kind| Body::text(kind, ""));
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
    let destination = directory.0.join("client.ts");
    std::fs::write(directory.0.join("package.json"), "{\"type\":\"module\"}").unwrap();
    volley_codegen::generate_file(&root, &destination, false, &Progress::default()).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/runtime.mjs");
    let output = Command::new("node").arg("--experimental-transform-types").arg(script).arg(&destination).output().expect("run Node.js 24");
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if let Some(compiler) = std::env::var_os("KESTREL_TYPESCRIPT_COMPILER") {
        let valid = directory.0.join("valid.ts");
        std::fs::write(&valid, include_str!("valid.ts")).unwrap();
        let output = Command::new("node")
            .arg(&compiler)
            .args(["--strict", "--noEmit", "--target", "ES2022", "--module", "NodeNext", "--allowImportingTsExtensions"])
            .arg(&valid)
            .output()
            .expect("typecheck generated SDK");
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        std::fs::write(&valid, "import { createClient } from './client.ts'; createClient().users.get({});").unwrap();
        let output = Command::new("node")
            .arg(compiler)
            .args(["--strict", "--noEmit", "--target", "ES2022", "--module", "NodeNext", "--allowImportingTsExtensions"])
            .arg(&valid)
            .output()
            .expect("check that missing path arguments are rejected");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("userId"));
    }
}
