use std::path::PathBuf;

use kestrel_openapi::{GroupBy, generate, load, parse};
use serde_json::json;

#[test]
fn openapi_32_imports_query_custom_methods_and_server_overrides() {
    let spec = json!({
        "openapi": "3.2.1", "info": {"title": "Search"},
        "servers": [{"url": "https://{region}.example.com", "variables": {"region": {"default": "eu"}}}],
        "components": {"parameters": {"Limit": {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 10}}}},
        "paths": {"/search": {
            "parameters": [{"$ref": "#/components/parameters/Limit"}],
            "get": {"summary": "Search"},
            "query": {"summary": "Query", "servers": [{"url": "https://query.example.com"}],
                "requestBody": {"content": {"application/json": {"schema": {"type": ["object", "null"], "properties": {"query": {"type": "string"}}}}}}},
            "additionalOperations": {"PROPFIND": {"summary": "Properties"}}
        }}
    });
    let generated = generate(&parse(&spec.to_string()).unwrap(), GroupBy::Tags);
    assert_eq!(generated.base_url.as_deref(), Some("https://eu.example.com"));
    assert_eq!(generated.operations.len(), 3);
    let query = generated.operations.iter().find(|operation| operation.file.http().method == "QUERY").unwrap();
    assert_eq!(query.file.http().url, "https://query.example.com/search");
    assert_eq!(query.file.http().params[0].value, "10");
    assert!(query.file.http().body.unwrap().as_text().contains("query"));
    assert_eq!(generated.operations[2].file.http().method, "PROPFIND");
}

#[test]
fn swagger_converts_inherited_parameters_bodies_responses_and_security() {
    let spec = json!({
        "swagger": "2.0", "info": {"title": "Pets"}, "host": "pets.example.com", "basePath": "/api", "schemes": ["https"],
        "consumes": ["application/json"], "produces": ["application/json"],
        "definitions": {"Pet": {"type": "object", "properties": {"name": {"type": "string", "example": "Milo"}}}},
        "parameters": {"PetId": {"name": "id", "in": "path", "required": true, "type": "integer"}},
        "securityDefinitions": {"Basic": {"type": "basic"}}, "security": [{"Basic": []}],
        "paths": {"/pets/{id}": {
            "parameters": [{"$ref": "#/parameters/PetId"}],
            "put": {"summary": "Update pet", "parameters": [
                {"name": "id", "in": "path", "required": true, "type": "integer", "default": 42},
                {"name": "body", "in": "body", "schema": {"$ref": "#/definitions/Pet"}}
            ], "responses": {"200": {"description": "A pet", "schema": {"$ref": "#/definitions/Pet"}, "examples": {"application/json": {"name": "Rex"}}}}},
            "post": {"summary": "Upload", "consumes": ["multipart/form-data"], "parameters": [{"name": "caption", "in": "formData", "type": "string", "example": "Hello"}]}
        }}
    });
    let generated = generate(&parse(&spec.to_string()).unwrap(), GroupBy::Tags);
    assert_eq!(generated.base_url.as_deref(), Some("https://pets.example.com/api"));
    let update = generated.operations.iter().find(|operation| operation.file.http().method == "PUT").unwrap();
    let http = update.file.http();
    assert_eq!(http.params.len(), 1);
    assert_eq!(http.params[0].value, "42");
    assert_eq!(http.auth.unwrap().mode(), "basic");
    assert!(http.body.unwrap().as_text().contains("Milo"));
    let examples = update.file.extra["examples"].as_sequence().unwrap();
    assert!(examples[0]["response"]["body"]["data"].as_str().unwrap().contains("Rex"));
    let upload = generated.operations.iter().find(|operation| operation.file.http().method == "POST").unwrap();
    assert_eq!(upload.file.http().body.unwrap().fields()[0].value, "Hello");
}

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kestrel-openapi-refs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove test spec files");
    }
}

#[test]
fn loads_external_path_items_and_recursive_schemas_relative_to_their_files() {
    let directory = Directory::new();
    std::fs::create_dir(directory.0.join("models")).unwrap();
    std::fs::write(
        directory.0.join("openapi.json"),
        json!({"openapi": "3.1.2", "info": {"title": "Pets"},
        "paths": {"/pets": {"$ref": "paths.yml#/pets"}}})
        .to_string(),
    )
    .unwrap();
    std::fs::write(directory.0.join("paths.yml"), "pets:\n  post:\n    summary: Create pet\n    requestBody:\n      content:\n        application/json:\n          schema:\n            $ref: models/pet.json#/Pet\n").unwrap();
    std::fs::write(
        directory.0.join("models/pet.json"),
        json!({"Pet": {"type": "object", "properties": {
            "name": {"type": "string", "example": "Milo"}, "friend": {"$ref": "#/Pet"}
        }}})
        .to_string(),
    )
    .unwrap();
    let spec = load("openapi.json", &directory.0).unwrap();
    let generated = generate(&spec, GroupBy::Tags);
    assert_eq!(generated.operations.len(), 1);
    let body = generated.operations[0].file.http().body.unwrap().as_text();
    assert!(body.contains("Milo"));
    assert!(body.contains("friend"));
    std::fs::remove_file(directory.0.join("models/pet.json")).unwrap();
    assert!(load("openapi.json", &directory.0).is_err());
}

#[test]
fn rejects_unknown_versions_and_malformed_swagger_without_panicking() {
    for document in [
        json!({"openapi": "3.3.0", "info": {}}),
        json!({"openapi": "30.0.0", "info": {}}),
        json!({"openapi": "3.2", "info": {}}),
        json!({"openapi": "3.2.1", "info": {}, "paths": {"/pets": null}}),
        json!({"swagger": "2.0", "info": {}, "paths": {"/pets": null}}),
        json!({"swagger": "2.0", "info": {}, "paths": {"/pets": {"get": "invalid"}}}),
    ] {
        assert!(parse(&document.to_string()).is_err(), "{document}");
    }
}
