#[path = "import_e2e/content.rs"]
mod content;
#[path = "import_e2e/export.rs"]
mod export;
#[path = "import_e2e/http.rs"]
mod http;
mod support;

use std::path::Path;

use volley_core::Collection;

use support::{Directory, assert_save_preserves_bytes, cases, edit_request, fixtures, snapshot, succeed};

fn import_examples(version: &str, extension: &str) {
    let directory = Directory::new();
    let selected: Vec<_> = cases().into_iter().filter(|case| case.version.starts_with(version) && case.path.ends_with(extension)).collect();
    assert!(!selected.is_empty(), "missing public examples for {version} {extension}");
    for (index, case) in selected.iter().enumerate() {
        let source = fixtures().join("upstream/oai").join(&case.path);
        for grouping in ["tags", "path"] {
            let destination = directory.0.join(format!("{index}-{grouping}"));
            let mut command = directory.cli();
            command.arg("import").arg(&source).arg("--out").arg(&destination).args(["--group-by", grouping]);
            if let Some(error) = &case.error {
                let output = command.output().unwrap();
                assert_eq!(output.status.code(), Some(2), "{case:?}");
                assert!(String::from_utf8_lossy(&output.stderr).contains(error), "{case:?}: {output:?}");
                assert!(!destination.exists(), "a rejected import created a collection: {case:?}");
                continue;
            }
            succeed(&mut command);
            let collection = Collection::open(&destination).unwrap();
            assert_eq!(collection.name(), case.title, "{case:?}");
            assert_eq!(collection.requests().len(), case.requests, "{case:?}");
            let sources = collection.doc.data.openapi_sources();
            assert_eq!(sources.len(), 1, "{case:?}");
            assert_eq!(sources[0].1, grouping);
            assert_eq!(destination.join(&sources[0].0).canonicalize().unwrap(), source.canonicalize().unwrap());
            assert_save_preserves_bytes(&destination);
            edit_request(&destination);
            let edited = snapshot(&destination);
            for flags in [&["--dry-run"][..], &[][..], &[][..]] {
                let output = succeed(directory.cli().arg("sync").arg(&destination).args(flags));
                let output = String::from_utf8(output.stdout).unwrap();
                assert!(output.contains(&format!("0 added, 0 updated, {} unchanged", case.requests)), "{case:?}: {output}");
                assert_eq!(snapshot(&destination), edited, "sync changed imported files or user edits: {case:?}");
            }
            let explicit = directory.0.join(format!("{index}-{grouping}-explicit"));
            succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&explicit).args([
                "--format",
                "openapi",
                "--group-by",
                grouping,
            ]));
            assert_eq!(Collection::open(&explicit).unwrap().requests().len(), case.requests);
        }
    }
}

#[test]
fn swagger_20_json() {
    import_examples("2.0", ".json");
}

#[test]
fn swagger_20_yaml() {
    import_examples("2.0", ".yaml");
}

#[test]
fn openapi_30_json() {
    import_examples("3.0", ".json");
}

#[test]
fn openapi_30_yaml() {
    import_examples("3.0", ".yaml");
}

#[test]
fn openapi_31_json() {
    import_examples("3.1", ".json");
}

#[test]
fn openapi_31_yaml() {
    import_examples("3.1", ".yaml");
}

#[test]
fn openapi_32_json() {
    import_examples("3.2", ".json");
}

#[test]
fn openapi_32_yaml() {
    import_examples("3.2", ".yaml");
}

#[test]
fn import_rejects_existing_collections_and_explicit_format_mismatches() {
    let directory = Directory::new();
    let source = fixtures().join("upstream/oai/v3.0/petstore.json");
    let destination = directory.0.join("existing");
    succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&destination));
    let original = snapshot(&destination);
    let output = directory.cli().arg("import").arg(&source).arg("--out").arg(&destination).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not an empty folder"));
    assert_eq!(snapshot(&destination), original);
    let mismatch = directory.0.join("mismatch");
    let output = directory.cli().arg("import").arg(&source).arg("--out").arg(&mismatch).args(["--format", "apidog"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!mismatch.exists());
    assert!(!snapshot(&directory.0).keys().any(|path| path.to_string_lossy().contains(".kestrel-import-")));
}

#[test]
fn native_apidog_wrapped_and_flat_exports_import_through_the_cli() {
    let directory = Directory::new();
    let wrapped = fixtures().join("apidog.json");
    let mut document: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&wrapped).unwrap()).unwrap();
    fn flatten(folder: &mut serde_json::Value) {
        let items = folder["items"].as_array_mut().unwrap();
        let mut children = Vec::new();
        let mut requests = Vec::new();
        for mut item in items.drain(..) {
            if item.get("items").is_some() {
                flatten(&mut item);
                children.push(item);
            } else {
                let mut api = item["api"].take();
                api["name"] = item["name"].take();
                requests.push(api);
            }
        }
        *items = requests;
        folder["children"] = children.into();
    }
    for folder in document["apiCollection"].as_array_mut().unwrap() {
        flatten(folder);
    }
    document["httpCollection"] = document["apiCollection"].take();
    document.as_object_mut().unwrap().remove("apiCollection");
    document["apidogProject"] = "1.1.0".into();
    let flat = directory.0.join("flat.json");
    std::fs::write(&flat, document.to_string()).unwrap();
    for (index, source) in [wrapped, flat].iter().enumerate() {
        for format in ["auto", "apidog"] {
            let destination = directory.0.join(format!("native-{index}-{format}"));
            let output = succeed(directory.cli().arg("import").arg(source).arg("--out").arg(&destination).args(["--format", format]));
            let warnings = String::from_utf8(output.stderr).unwrap();
            assert!(warnings.contains("Environments are not imported"));
            assert!(warnings.contains("processors are not imported"));
            let collection = Collection::open(&destination).unwrap();
            assert_eq!(collection.requests().len(), 4);
            assert!(collection.doc.data.openapi_sources().is_empty());
            assert_save_preserves_bytes(&destination);
            assert!(collection.requests().iter().any(|request| request.doc.path.ends_with(Path::new("Pets/Details/Get pet.yml"))));
            for bytes in snapshot(&destination).values() {
                let text = String::from_utf8_lossy(bytes);
                assert!(!text.contains("must-not-be-written"));
                assert!(!text.contains("pm.variables.set"));
            }
        }
    }
}
