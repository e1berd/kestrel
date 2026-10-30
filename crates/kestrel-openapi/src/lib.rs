use std::path::Path;

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use kestrel_core::format::RequestFile;
use serde_json::Value as Json;

mod generate;
mod loading;
mod samples;
mod swagger;
mod sync;

pub use generate::{generate, request_key};
pub use sync::{Report, SyncOptions, import, sync};

pub(crate) const METHODS: [&str; 9] = ["get", "post", "put", "patch", "delete", "head", "options", "trace", "query"];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Tags,
    Path,
}
impl GroupBy {
    pub fn parse(text: &str) -> Self {
        if text == "path" { GroupBy::Path } else { GroupBy::Tags }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GroupBy::Tags => "tags",
            GroupBy::Path => "path",
        }
    }
}
pub fn load(source: &str, base: &Path) -> Result<Json> {
    normalize(loading::load(source, base)?)
}
pub fn parse(text: &str) -> Result<Json> {
    let text = text.trim_start_matches('\u{feff}');
    let spec: Json = match serde_json::from_str(text) {
        Ok(json) => json,
        Err(_) => serde_yaml_ng::from_str(text).context("the spec is neither JSON nor YAML")?,
    };
    normalize(spec)
}

pub fn normalize(spec: Json) -> Result<Json> {
    match spec.get("openapi").and_then(Json::as_str) {
        Some(version) if supported_version(version) => {
            if !spec.get("info").is_some_and(Json::is_object) {
                bail!("the OpenAPI spec must contain an `info` object");
            }
            if let Some(paths) = spec.get("paths")
                && !paths.is_object()
            {
                bail!("OpenAPI `paths` must be an object");
            }
            for (path, item) in spec.get("paths").and_then(Json::as_object).into_iter().flatten() {
                if path.starts_with('/') && !item.is_object() {
                    bail!("OpenAPI path item `{path}` must be an object");
                }
            }
            Ok(spec)
        }
        Some(version) => bail!("OpenAPI {version} is not supported; use 3.0, 3.1, 3.2 or Swagger 2.0"),
        None if spec.get("swagger").and_then(Json::as_str) == Some("2.0") => swagger::convert(spec),
        None if spec.get("swagger").is_some() => bail!("only Swagger 2.0 is supported"),
        None => bail!("that is not an OpenAPI spec (it has no `openapi` field)"),
    }
}

fn supported_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3 && parts[0] == "3" && matches!(parts[1], "0" | "1" | "2") && parts[2].parse::<u32>().is_ok()
}
#[derive(Debug, Clone)]
pub struct Operation {
    pub folder: Option<String>,
    pub file: RequestFile,
    pub key: String,
}
#[derive(Debug, Clone)]
pub struct Generated {
    pub title: String,
    pub base_url: Option<String>,
    pub operations: Vec<Operation>,
    pub folder_docs: IndexMap<String, String>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_core::format::Body;

    const SPEC: &str = r##"{
      "openapi": "3.1.0",
      "info": {"title": "Pets"},
      "components": {
        "securitySchemes": {"B": {"type": "http", "scheme": "bearer"}},
        "schemas": {"Pet": {"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}, "tags": {"type": "array", "items": {"type": "string"}}}}}
      },
      "security": [{"B": []}],
      "paths": {
        "/pets/{pet_id}": {
          "get": {"summary": "Get Pet", "tags": ["pets"], "parameters": [{"in": "path", "name": "pet_id", "required": true}, {"in": "query", "name": "full", "schema": {"type": "boolean", "default": false}}, {"in": "query", "name": "q"}],
                  "responses": {"200": {"description": "OK", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}}}},
          "patch": {"summary": "Update Pet: partly", "tags": ["pets"], "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}}, "responses": {"204": {"description": "Done"}}}
        }
      }
    }"##;

    #[test]
    fn operations_become_requests() {
        let spec = parse(SPEC).unwrap();
        let generated = generate(&spec, GroupBy::Tags);
        assert_eq!(generated.operations.len(), 2);
        let get = &generated.operations[0];
        assert_eq!(get.folder.as_deref(), Some("pets"));
        assert_eq!(get.key, "GET /pets/{}");
        let http = get.file.http();
        assert_eq!(http.url, "{{baseUrl}}/pets/:pet_id");
        assert_eq!(
            http.params.iter().map(|p| (p.name.as_str(), p.value.as_str(), p.disabled)).collect::<Vec<_>>(),
            vec![("pet_id", "", false), ("full", "false", false), ("q", "", true)]
        );
        assert_eq!(http.auth.unwrap().mode(), "bearer");
        let patch = generated.operations[1].file.http();
        assert_eq!(patch.body.unwrap().as_text(), "{\n  \"age\": 0,\n  \"name\": \"\",\n  \"tags\": []\n}");
        assert_eq!(request_key(&generated.operations[1].file).as_deref(), Some("PATCH /pets/{}"));
    }

    #[test]
    fn import_then_sync_keeps_edits() {
        let dir = std::env::temp_dir().join(format!("kestrel-openapi-{}", uuid::Uuid::new_v4()));
        let spec = parse(SPEC).unwrap();
        let (mut collection, report) = import(&dir, "spec.json", &spec, GroupBy::Tags, None).unwrap();
        assert_eq!(report.added.len(), 2);
        let path = collection.requests().iter().find(|r| r.method() == "PATCH").unwrap().doc.path.clone();
        assert!(path.ends_with("pets/Update Pet- partly.yml"));
        {
            let request = collection.request_mut(&path).unwrap();
            request.doc.data.http.as_mut().unwrap().body = Some(Body::text("json", "{\"name\": \"Rex\"}"));
            request.doc.save().unwrap();
        }
        let report = sync(&mut collection, &spec, &SyncOptions { group_by: GroupBy::Tags, prune: false, dry_run: false }).unwrap();
        assert_eq!((report.added.len(), report.updated.len(), report.unchanged), (0, 0, 2));
        let body = collection.request(&path).unwrap().doc.data.http().body.unwrap().as_text();
        assert_eq!(body, "{\"name\": \"Rex\"}");
        assert_eq!(collection.doc.data.openapi_sources(), vec![("spec.json".to_string(), "tags".to_string())]);
        std::fs::remove_dir_all(&dir).expect("remove the test collection");
    }
}
