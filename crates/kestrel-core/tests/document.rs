use std::path::PathBuf;

use kestrel_core::Document;
use kestrel_core::format::RequestFile;
use serde_yaml_ng::Value;

const SOURCE: &str = "info:\n  name: Snapshot\n  type: http\n\nhttp:\n  method: GET\n  url: /original\n\nfuture:\n  nested:\n    - enabled: true\n      value: preserve\n";

#[test]
fn a_document_keeps_its_original_contents_without_reopening_the_file() {
    let mut document = Document::<RequestFile>::parse(&PathBuf::from("missing.yml"), SOURCE, true).unwrap();
    assert_eq!(document.render().unwrap(), SOURCE);
    assert!(!document.is_dirty());
    document.data.http.as_mut().unwrap().url = "/edited".into();
    document.data.extra.insert("another".into(), Value::Bool(true));
    assert!(document.is_dirty());
    document.revert();
    assert_eq!(document.render().unwrap(), SOURCE);
    assert!(!document.is_dirty());
}

#[test]
fn cloned_documents_keep_independent_saved_snapshots_and_unknown_fields() {
    let directory = std::env::temp_dir().join(format!("kestrel-document-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("request.yml");
    std::fs::write(&path, SOURCE).unwrap();
    let original = Document::<RequestFile>::read(&path, true).unwrap();
    let mut edited = original.clone();
    edited.data.http.as_mut().unwrap().url = "/edited".into();
    edited.save().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), SOURCE.replace("/original", "/edited"));
    assert_eq!(original.render().unwrap(), SOURCE);
    assert!(!original.is_dirty());
    assert!(!edited.is_dirty());
    edited.data.http.as_mut().unwrap().url = "/unsaved".into();
    edited.revert();
    assert_eq!(edited.data.http.unwrap().url, "/edited");
    assert_eq!(edited.data.extra, original.data.extra);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_new_document_can_be_saved_and_reverted_using_its_saved_source() {
    let path = std::env::temp_dir().join(format!("kestrel-new-document-{}.yml", uuid::Uuid::new_v4()));
    let mut document = Document::new(path.clone(), RequestFile::new_http("New", "GET", "/initial"), true);
    assert!(document.is_dirty());
    document.save().unwrap();
    assert!(!document.is_dirty());
    document.data.http.as_mut().unwrap().url = "/edited".into();
    assert!(document.is_dirty());
    document.revert();
    assert_eq!(document.data.http.unwrap().url, "/initial");
    std::fs::remove_file(path).unwrap();
}
