use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use volley_core::transfer::Progress;
use volley_import::PreparedImport;
use volley_import::export::{Encoding, ExportFormat, export};
use volley_openapi::GroupBy;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kestrel-stream-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn generate(path: &Path, count: usize) {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    file.write_all(br#"{"openapi":"3.2.1","info":{"title":"Large"},"paths":{"#).unwrap();
    for index in 0..count {
        if index > 0 {
            file.write_all(b",").unwrap();
        }
        let path = format!("/items/{index}");
        serde_json::to_writer(&mut file, &path).unwrap();
        file.write_all(b":").unwrap();
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({"post": {"summary": format!("Request {index}"),
            "requestBody": {"content": {"application/json": {"example": {"index": index, "payload": "x".repeat(4096)}}}}}}),
        )
        .unwrap();
    }
    file.write_all(b"}}").unwrap();
    file.flush().unwrap();
}

#[test]
fn cancelling_during_import_removes_the_partially_written_collection() {
    let directory = Directory::new();
    let input = directory.0.join("input.json");
    generate(&input, 1000);
    let prepared = PreparedImport::load(&input.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
    let destination = directory.0.join("cancelled");
    let progress = Progress::default();
    let observed = progress.clone();
    let path = destination.clone();
    let worker = thread::spawn(move || prepared.save_with_progress(&path, None, &progress));
    let mut cancelled = false;
    while !worker.is_finished() {
        let state = observed.snapshot();
        if state.stage == "Writing requests" && state.completed > 0 && state.completed < state.total.unwrap() {
            assert!(state.percent().is_some_and(|percent| percent > 0. && percent < 100.));
            observed.cancel();
            cancelled = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(worker.join().unwrap().is_err());
    assert!(cancelled);
    assert!(!observed.snapshot().finished);
    assert!(!destination.exists());
    assert!(
        !std::fs::read_dir(&directory.0).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".kestrel-import-"))
    );
}

#[test]
fn cancelling_during_export_preserves_the_previous_file() {
    let directory = Directory::new();
    let input = directory.0.join("input.json");
    generate(&input, 1000);
    let prepared = PreparedImport::load(&input.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
    let root = prepared.save_with_progress(&directory.0.join("collection"), None, &Progress::default()).unwrap();
    let destination = directory.0.join("export.json");
    std::fs::write(&destination, "keep previous export").unwrap();
    let progress = Progress::default();
    let observed = progress.clone();
    let path = destination.clone();
    let worker = thread::spawn(move || export(&root, &path, ExportFormat::OpenApi32, Encoding::Json, true, &progress));
    let mut cancelled = false;
    while !worker.is_finished() {
        let state = observed.snapshot();
        if state.stage == "Writing export" && state.completed > 0 && state.completed < state.total.unwrap() {
            observed.cancel();
            cancelled = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(worker.join().unwrap().is_err());
    assert!(cancelled);
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "keep previous export");
    assert!(!observed.snapshot().finished);
    assert!(
        !std::fs::read_dir(&directory.0).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".kestrel-export-"))
    );
}

#[test]
fn yaml_aliases_unicode_and_late_schema_definitions_are_resolved() {
    let input = r##"openapi: 3.2.1
info:
  title: "Большая схема 🌍"
paths:
  /one:
    post: &operation
      summary: Создать
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/Message'
  /two:
    post: *operation
components:
  schemas:
    Message:
      type: object
      properties:
        text:
          type: string
          example: "Привет 🌍"
"##;
    let directory = Directory::new();
    let prepared = PreparedImport::parse(input, "input.yaml", None, GroupBy::Tags).unwrap();
    assert_eq!(prepared.preview.requests, 2);
    let collection = prepared.save(&directory.0.join("yaml"), None).unwrap();
    assert_eq!(collection.name(), "Большая схема 🌍");
    for request in collection.requests() {
        assert!(request.doc.data.http().body.unwrap().as_text().contains("Привет 🌍"));
    }
}
