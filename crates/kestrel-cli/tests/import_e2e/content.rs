use kestrel_core::Collection;
use serde_json::Value as Json;

use super::support::{Directory, cases, fixtures, succeed};

#[test]
fn public_examples_keep_every_outgoing_method_and_path() {
    let directory = Directory::new();
    for (index, case) in cases().iter().filter(|case| case.error.is_none()).enumerate() {
        let source = fixtures().join("upstream/oai").join(&case.path);
        let document: Json = serde_yaml_ng::from_str(&std::fs::read_to_string(&source).unwrap()).unwrap();
        let destination = directory.0.join(index.to_string());
        succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&destination));
        let collection = Collection::open(&destination).unwrap();
        for (path, item) in document["paths"].as_object().unwrap() {
            for method in ["get", "post", "put", "patch", "delete", "head", "options", "trace", "query"] {
                if item.get(method).is_none() {
                    continue;
                }
                let expected = path.replace('{', ":").replace('}', "");
                assert!(
                    collection.requests().iter().any(|request| {
                        let http = request.doc.data.http();
                        http.method == method.to_uppercase() && http.url.ends_with(&expected)
                    }),
                    "missing {method} {path} from {}",
                    case.path
                );
            }
        }
    }
}

#[test]
fn response_example_payloads_survive_swagger_and_openapi_imports() {
    let directory = Directory::new();
    for (index, source) in [
        "v2.0/json/api-with-examples.json",
        "v2.0/yaml/api-with-examples.yaml",
        "v3.0/api-with-examples.json",
        "v3.0/api-with-examples.yaml",
    ]
    .iter()
    .enumerate()
    {
        let source = fixtures().join("upstream/oai").join(source);
        let destination = directory.0.join(index.to_string());
        succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&destination));
        let collection = Collection::open(&destination).unwrap();
        let requests = collection.requests();
        let listing = requests.iter().find(|request| request.doc.data.http().url.ends_with('/')).unwrap();
        let examples = listing.doc.data.extra["examples"].as_sequence().unwrap();
        assert_eq!(examples.len(), 2);
        for example in examples {
            let body = example["response"]["body"]["data"].as_str().unwrap();
            let body: Json = serde_json::from_str(body).unwrap_or_else(|error| panic!("{}: {error}: {body}", source.display()));
            assert_eq!(body["versions"].as_array().unwrap().len(), 2, "{}", source.display());
            assert_eq!(body["versions"][0]["id"], "v2.0");
        }
    }
}

#[test]
fn external_swagger_schema_composition_keeps_all_properties() {
    let directory = Directory::new();
    for (index, extension) in ["json", "yaml"].iter().enumerate() {
        let source = fixtures().join(format!("upstream/oai/v2.0/{extension}/petstore-separate/spec/swagger.{extension}"));
        let destination = directory.0.join(index.to_string());
        succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&destination));
        let collection = Collection::open(&destination).unwrap();
        let requests = collection.requests();
        let post = requests.iter().find(|request| request.method() == "POST").unwrap();
        let body: Json = serde_json::from_str(&post.doc.data.http().body.unwrap().as_text()).unwrap();
        for property in ["id", "name", "tag", "description"] {
            assert!(body.get(property).is_some(), "lost external allOf property {property}: {body}");
        }
    }
}

#[test]
fn tic_tac_toe_keeps_shared_parameters_and_authentication() {
    let directory = Directory::new();
    let source = fixtures().join("upstream/oai/v3.1/tictactoe.yaml");
    let destination = directory.0.join("tictactoe");
    succeed(directory.cli().arg("import").arg(&source).arg("--out").arg(&destination));
    let collection = Collection::open(&destination).unwrap();
    let requests = collection.requests();
    let board = requests.iter().find(|request| request.doc.data.http().url.ends_with("/board")).unwrap();
    assert_eq!(board.doc.data.http().auth.unwrap().mode(), "apikey");
    let square = requests.iter().find(|request| request.method() == "PUT").unwrap();
    let http = square.doc.data.http();
    for name in ["row", "column"] {
        assert!(
            http.params.iter().any(|parameter| parameter.name == name && parameter.kind.as_deref() == Some("path") && !parameter.disabled)
        );
    }
    assert!(http.headers.iter().any(|header| header.name == "progressUrl"));
    assert_eq!(http.body.unwrap().as_text(), "\".\"");
}
