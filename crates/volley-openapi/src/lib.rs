use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use serde_json::Value as Json;
use volley_core::format::RequestFile;

mod generate;
mod samples;
mod sync;

pub use generate::{generate, request_key};
pub use sync::{Report, SyncOptions, import, sync};

pub(crate) const METHODS: [&str; 8] = ["get", "post", "put", "patch", "delete", "head", "options", "trace"];
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
    let text = if source.starts_with("http://") || source.starts_with("https://") {
        let response = reqwest::blocking::get(source).with_context(|| format!("download {source}"))?;
        if !response.status().is_success() {
            bail!("{source} answered {}", response.status());
        }
        response.text()?
    } else {
        let path = if Path::new(source).is_absolute() { PathBuf::from(source) } else { base.join(source) };
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?
    };
    parse(&text)
}
pub fn parse(text: &str) -> Result<Json> {
    let spec: Json = match serde_json::from_str(text) {
        Ok(json) => json,
        Err(_) => serde_yaml_ng::from_str(text).context("the spec is neither JSON nor YAML")?,
    };
    match spec.get("openapi").and_then(Json::as_str) {
        Some(v) if v.starts_with('3') => Ok(spec),
        Some(v) => bail!("OpenAPI {v} is not supported; 3.0 and 3.1 are"),
        None if spec.get("swagger").is_some() => {
            bail!("Swagger 2.0 is not supported yet; convert it to OpenAPI 3 first")
        }
        None => bail!("that is not an OpenAPI spec (it has no `openapi` field)"),
    }
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
    use volley_core::format::Body;

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
        std::fs::remove_dir_all(&dir).ok();
    }
}
