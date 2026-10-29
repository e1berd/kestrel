use volley_core::Collection;

use super::support::{Directory, fixtures, snapshot, succeed};

#[test]
fn imported_openapi_generates_resource_sdk_without_changing_the_collection() {
    let directory = Directory::new();
    let root = directory.0.join("collection");
    succeed(directory.cli().arg("import").arg(fixtures().join("upstream/oai/v3.0/petstore-expanded.yaml")).arg("--out").arg(&root));
    let before = snapshot(&root);
    let destination = directory.0.join("sdk/client.ts");
    let output = succeed(directory.cli().arg("codegen").arg(&root).arg("--out").arg(&destination));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pets.list"));
    assert!(stdout.contains("pets.get"));
    assert!(stdout.contains("pets.insert"));
    assert!(stdout.contains("pets.delete"));
    let source = std::fs::read(&destination).unwrap();
    assert_eq!(snapshot(&root), before);
    let output = directory.cli().arg("codegen").arg(&root).arg("--out").arg(&destination).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(&destination).unwrap(), source);
    succeed(directory.cli().arg("codegen").arg(&root).arg("--out").arg(&destination).arg("--overwrite"));
    assert_eq!(std::fs::read(&destination).unwrap(), source);
}

#[test]
fn apidog_duplicate_requests_report_conflicts_and_allow_explicit_method_names() {
    let directory = Directory::new();
    let root = directory.0.join("collection");
    succeed(directory.cli().arg("import").arg(fixtures().join("apidog.json")).arg("--out").arg(&root));
    let destination = directory.0.join("client.ts");
    let output = directory.cli().arg("codegen").arg(&root).arg("--out").arg(&destination).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("pets.insert"));
    assert!(!destination.exists());
    let mut collection = Collection::open(&root).unwrap();
    let path = collection
        .requests()
        .into_iter()
        .filter(|request| request.method() == "POST" && request.doc.data.http().url.ends_with("/pets"))
        .nth(1)
        .unwrap()
        .doc
        .path
        .clone();
    let request = collection.request_mut(&path).unwrap();
    request
        .doc
        .data
        .extra
        .insert("extensions".into(), serde_yaml_ng::from_str("kestrel:\n  codegen:\n    method: insertExample\n").unwrap());
    request.doc.save().unwrap();
    let output = succeed(directory.cli().arg("codegen").arg(&root).arg("--out").arg(&destination));
    let stdout = String::from_utf8(output.stdout).unwrap();
    for method in ["pets.get", "pets.insert", "pets.insertExample", "upload.insert"] {
        assert!(stdout.contains(method), "{stdout}");
    }
    assert!(!std::fs::read_to_string(destination).unwrap().contains("must-not-be-written"));
}
