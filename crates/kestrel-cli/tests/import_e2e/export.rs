use kestrel_core::Collection;

use super::support::{Directory, cases, fixtures, succeed};

#[test]
fn export_cli_supports_all_formats_and_encodings() {
    let directory = Directory::new();
    let root = directory.0.join("collection");
    succeed(directory.cli().arg("import").arg(fixtures().join("upstream/oai/v3.0/petstore-expanded.yaml")).arg("--out").arg(&root));
    for format in ["openapi30", "openapi31", "openapi32", "swagger20", "apidog"] {
        for encoding in ["json", "yaml"] {
            if format == "apidog" && encoding == "yaml" {
                continue;
            }
            let path = directory.0.join(format!("{format}.{encoding}"));
            succeed(directory.cli().arg("export").arg(&root).arg("--out").arg(&path).args(["--format", format, "--encoding", encoding]));
            let restored = directory.0.join(format!("restored-{format}-{encoding}"));
            succeed(directory.cli().arg("import").arg(&path).arg("--out").arg(&restored));
            assert_eq!(Collection::open(&restored).unwrap().requests().len(), 4);
            let before = std::fs::read(&path).unwrap();
            let output = directory.cli().arg("export").arg(&root).arg("--out").arg(&path).output().unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
}

#[test]
fn public_corpus_survives_openapi_and_apidog_export_reimport() {
    let directory = Directory::new();
    for (index, case) in cases().iter().filter(|case| case.error.is_none()).enumerate() {
        let source = fixtures().join("upstream/oai").join(&case.path);
        let root = directory.0.join(format!("source-{index}"));
        succeed(directory.cli().arg("import").arg(source).arg("--out").arg(&root));
        for format in ["openapi32", "apidog"] {
            let output = directory.0.join(format!("{index}-{format}.json"));
            succeed(directory.cli().arg("export").arg(&root).arg("--out").arg(&output).args(["--format", format]));
            let restored = directory.0.join(format!("restored-{index}-{format}"));
            succeed(directory.cli().arg("import").arg(output).arg("--out").arg(&restored));
            assert_eq!(Collection::open(&restored).unwrap().requests().len(), case.requests, "{case:?} {format}");
        }
    }
}
