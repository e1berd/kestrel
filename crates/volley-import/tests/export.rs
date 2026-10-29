use std::path::{Path, PathBuf};

use serde_json::Value as Json;
use volley_core::format::{Body, KeyValue, RequestFile};
use volley_core::transfer::{Progress, Unit};
use volley_core::{Collection, Document};
use volley_import::PreparedImport;
use volley_import::export::{Encoding, ExportFormat, export};
use volley_openapi::GroupBy;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kestrel-export-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn source(directory: &Directory) -> PathBuf {
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/upstream/oai/v3.0/petstore-expanded.json");
    let prepared = PreparedImport::load(&input.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
    prepared.save_with_progress(&directory.0.join("source"), None, &Progress::default()).unwrap()
}

#[test]
fn every_export_format_and_encoding_can_be_imported_again() {
    let directory = Directory::new();
    let source = source(&directory);
    let original = Collection::open(&source).unwrap();
    for format in ExportFormat::ALL {
        for encoding in [Encoding::Json, Encoding::Yaml] {
            if format == ExportFormat::Apidog && encoding == Encoding::Yaml {
                continue;
            }
            let progress = Progress::default();
            let destination = directory.0.join(format!("{}-{encoding:?}", format.id()));
            let report = export(&source, &destination, format, encoding, false, &progress).unwrap();
            assert_eq!(report.requests, original.requests().len());
            assert_eq!(report.bytes, std::fs::metadata(&destination).unwrap().len());
            assert!(progress.snapshot().finished);
            let prepared = PreparedImport::load(&destination.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
            assert_eq!(prepared.preview.requests, original.requests().len(), "{format:?} {encoding:?}");
            let restored = prepared.save(&directory.0.join(format!("restored-{}-{encoding:?}", format.id())), None).unwrap();
            assert_eq!(restored.name(), original.name());
            assert_eq!(restored.defaults().variables, original.defaults().variables);
            for request in original.requests() {
                let requests = restored.requests();
                let restored =
                    requests.iter().find(|candidate| candidate.name() == request.name() && candidate.method() == request.method()).unwrap();
                let before = request.doc.data.http();
                let after = restored.doc.data.http();
                assert_eq!(after.url, before.url, "{format:?}: {}", request.name());
                for parameter in &before.params {
                    assert!(
                        after.params.iter().any(|candidate| candidate.name == parameter.name
                            && candidate.value == parameter.value
                            && candidate.disabled == parameter.disabled),
                        "{format:?}: lost {parameter:?}"
                    );
                }
                if let Some(body) = before.body {
                    let expected: Json = serde_json::from_str(&body.as_text()).unwrap();
                    let actual: Json = serde_json::from_str(&after.body.unwrap().as_text()).unwrap();
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}

#[test]
fn xml_bodies_and_disabled_headers_survive_export() {
    let directory = Directory::new();
    let source = directory.0.join("source");
    Collection::create(&source, "XML").unwrap();
    let mut request = RequestFile::new_http("XML", "POST", "{{baseUrl}}/xml");
    let http = request.http.as_mut().unwrap();
    http.body = Some(Body::text("xml", "<message>Привет 🌍</message>"));
    let mut header = KeyValue::new("X-Disabled", "off");
    header.disabled = true;
    http.headers.push(header);
    Document::new(source.join("XML.yml"), request, true).save().unwrap();
    for format in ExportFormat::ALL {
        let destination = directory.0.join(format.id());
        export(&source, &destination, format, Encoding::Json, false, &Progress::default()).unwrap();
        let prepared = PreparedImport::load(&destination.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
        let collection = prepared.save(&directory.0.join(format!("restored-{}", format.id())), None).unwrap();
        let http = collection.requests()[0].doc.data.http();
        assert_eq!(http.body.unwrap().as_text(), "<message>Привет 🌍</message>");
        assert!(http.headers.iter().any(|header| header.name == "X-Disabled" && header.disabled));
    }
}

#[test]
fn duplicate_openapi_operations_and_failed_writes_preserve_existing_output() {
    let directory = Directory::new();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/apidog.json");
    let prepared = PreparedImport::load(&input.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
    let source = prepared.save_with_progress(&directory.0.join("source"), None, &Progress::default()).unwrap();
    let output = directory.0.join("output.json");
    std::fs::write(&output, "keep this file").unwrap();
    let progress = Progress::default();
    assert!(export(&source, &output, ExportFormat::OpenApi32, Encoding::Json, true, &progress).is_err());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "keep this file");
    assert!(!progress.snapshot().finished);
    export(&source, &output, ExportFormat::Apidog, Encoding::Json, true, &Progress::default()).unwrap();
    assert_eq!(PreparedImport::load(&output.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap().preview.requests, 4);
    assert!(
        !std::fs::read_dir(&directory.0).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".kestrel-export-"))
    );
}

#[test]
fn cancellation_and_existing_destinations_do_not_publish_output() {
    let directory = Directory::new();
    let source = source(&directory);
    let destination = directory.0.join("cancelled.json");
    let progress = Progress::default();
    progress.cancel();
    assert!(export(&source, &destination, ExportFormat::OpenApi32, Encoding::Json, false, &progress).is_err());
    assert!(!destination.exists());
    std::fs::write(&destination, "keep").unwrap();
    assert!(export(&source, &destination, ExportFormat::OpenApi32, Encoding::Json, false, &Progress::default()).is_err());
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "keep");
}

#[test]
fn byte_progress_counts_consumed_input_and_never_invents_a_total() {
    use std::io::Read as _;
    let progress = Progress::default();
    progress.start("Reading", Some(4096), Unit::Bytes).unwrap();
    let mut reader = progress.reader(std::io::Cursor::new(vec![1; 4096]));
    let mut buffer = [0; 1024];
    reader.read_exact(&mut buffer).unwrap();
    assert_eq!(progress.snapshot().completed, 1024);
    assert_eq!(progress.snapshot().percent(), Some(25.));
    assert!(!progress.snapshot().finished);
    assert!(progress.finish().is_err());
    progress.start("Downloading", None, Unit::Bytes).unwrap();
    reader.read_exact(&mut buffer).unwrap();
    assert_eq!(progress.snapshot().completed, 1024);
    assert_eq!(progress.snapshot().percent(), None);
    progress.cancel();
    assert!(reader.read(&mut buffer).is_err());
}

#[test]
fn multipart_file_fields_and_disabled_values_survive_export() {
    let directory = Directory::new();
    let source = directory.0.join("source");
    Collection::create(&source, "Upload").unwrap();
    let mut request = RequestFile::new_http("Upload", "POST", "{{baseUrl}}/upload");
    let mut body = Body { kind: "multipart-form".into(), ..Default::default() };
    let mut file = KeyValue::new("attachment", "{{process.env.FILE}}");
    file.kind = Some("file".into());
    let mut caption = KeyValue::new("caption", "off");
    caption.disabled = true;
    body.set_fields(&[file, caption]);
    request.http.as_mut().unwrap().body = Some(body);
    Document::new(source.join("Upload.yml"), request, true).save().unwrap();
    for format in ExportFormat::ALL {
        let destination = directory.0.join(format.id());
        export(&source, &destination, format, Encoding::Json, false, &Progress::default()).unwrap();
        let prepared = PreparedImport::load(&destination.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
        let collection = prepared.save(&directory.0.join(format!("restored-{}", format.id())), None).unwrap();
        let fields = collection.requests()[0].doc.data.http().body.unwrap().fields();
        assert!(
            fields
                .iter()
                .any(|field| field.name == "attachment" && field.kind.as_deref() == Some("file") && field.value == "{{process.env.FILE}}"),
            "{format:?}: {fields:?}"
        );
        assert!(fields.iter().any(|field| field.name == "caption" && field.disabled), "{format:?}: {fields:?}");
    }
}

#[test]
fn collection_variables_survive_without_exposing_secret_marked_values() {
    let directory = Directory::new();
    let source = source(&directory);
    let mut collection = Collection::open(&source).unwrap();
    let defaults = collection.doc.data.request.as_mut().unwrap();
    defaults.variables.push(KeyValue::new("user", "Пример"));
    let mut secret = KeyValue::new("api_token", "sentinel-not-a-real-secret");
    secret.extra.insert("secret".into(), serde_yaml_ng::Value::Bool(true));
    defaults.variables.push(secret);
    collection.doc.save().unwrap();
    for format in ExportFormat::ALL {
        let path = directory.0.join(format.id());
        export(&source, &path, format, Encoding::Json, false, &Progress::default()).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("sentinel-not-a-real-secret"));
        let prepared = PreparedImport::load(&path.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
        let restored = prepared.save(&directory.0.join(format!("restored-{}", format.id())), None).unwrap();
        let variables = restored.defaults().variables;
        assert!(variables.iter().any(|variable| variable.name == "user" && variable.value == "Пример"));
        assert!(variables.iter().any(|variable| variable.name == "api_token" && variable.value == "{{process.env.API_TOKEN}}"));
    }
}

#[test]
fn swagger_uses_absolute_request_hosts_and_rejects_multiple_origins() {
    let directory = Directory::new();
    let source = directory.0.join("source");
    Collection::create(&source, "Absolute URLs").unwrap();
    Document::new(source.join("Pets.yml"), RequestFile::new_http("Pets", "GET", "https://api.example.com/v1/pets"), true).save().unwrap();
    let output = directory.0.join("swagger.json");
    export(&source, &output, ExportFormat::Swagger20, Encoding::Json, false, &Progress::default()).unwrap();
    let document: Json = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(document["host"], "api.example.com");
    let prepared = PreparedImport::load(&output.to_string_lossy(), Path::new("/"), None, GroupBy::Tags).unwrap();
    let restored = prepared.save(&directory.0.join("restored"), None).unwrap();
    let base = restored.defaults().variables.into_iter().find(|variable| variable.name == "baseUrl").unwrap().value;
    assert_eq!(restored.requests()[0].doc.data.http().url.replace("{{baseUrl}}", &base), "https://api.example.com/v1/pets");
    Document::new(source.join("Other.yml"), RequestFile::new_http("Other", "GET", "https://other.example.com/items"), true).save().unwrap();
    let before = std::fs::read(&output).unwrap();
    assert!(export(&source, &output, ExportFormat::Swagger20, Encoding::Json, true, &Progress::default()).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), before);
}
