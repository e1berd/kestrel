use serde::ser::{Error as _, SerializeMap as _, SerializeSeq as _};
use serde::{Serialize, Serializer};
use serde_json::json;
use volley_core::format::RequestFile;
use volley_core::transfer::Progress;

use super::index::{Index, read};
use super::{ExportFormat, native, operation};

pub(super) struct Document<'a> {
    pub index: &'a Index,
    pub format: ExportFormat,
    pub progress: &'a Progress,
}

impl Serialize for Document<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        let mut variables: Vec<_> = self
            .index
            .collection
            .request
            .as_ref()
            .into_iter()
            .flat_map(|defaults| &defaults.variables)
            .map(|variable| {
                let mut variable = variable.clone();
                if variable.is_secret() {
                    let name: String = variable
                        .name
                        .chars()
                        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_uppercase() } else { '_' })
                        .collect();
                    variable.value = format!("{{{{process.env.{name}}}}}");
                }
                variable
            })
            .collect();
        if self.format == ExportFormat::Swagger20
            && let Some(base) = &self.index.swagger_base
        {
            for variable in &mut variables {
                if variable.name == "baseUrl" {
                    variable.value = base.clone();
                }
            }
        }
        if !variables.is_empty() {
            map.serialize_entry("x-kestrel-variables", &variables)?;
        }
        let title = &self.index.collection.info.name;
        let base = self
            .index
            .swagger_base
            .as_deref()
            .or_else(|| {
                variables.iter().find(|variable| variable.name == "baseUrl" && !variable.disabled).map(|variable| variable.value.as_str())
            })
            .unwrap_or("http://localhost:8000");
        if self.format == ExportFormat::Apidog {
            map.serialize_entry("apidogProject", self.format.version())?;
            map.serialize_entry(
                "info",
                &json!({"name": title, "description": self.index.collection.docs.as_ref().map(|docs| docs.content()).unwrap_or("")}),
            )?;
            map.serialize_entry("x-kestrel-base-url", base)?;
            map.serialize_entry("apiCollection", &NativeRoots(self))?;
        } else {
            map.serialize_entry(if self.format == ExportFormat::Swagger20 { "swagger" } else { "openapi" }, self.format.version())?;
            let mut info = json!({"title": title, "version": "1.0.0"});
            if let Some(docs) = &self.index.collection.docs {
                info["description"] = json!(docs.content());
            }
            map.serialize_entry("info", &info)?;
            if self.format == ExportFormat::Swagger20 {
                let url = url::Url::parse(base).map_err(S::Error::custom)?;
                let authority = &url[url::Position::BeforeHost..url::Position::AfterPort];
                map.serialize_entry("host", authority)?;
                map.serialize_entry("basePath", if url.path() == "/" { "" } else { url.path() })?;
                map.serialize_entry("schemes", &[url.scheme()])?;
                map.serialize_entry("produces", &["application/json"])?;
                map.serialize_entry(
                    "securityDefinitions",
                    &json!({"basic": {"type": "basic"}, "bearer": {"type": "apiKey", "in": "header", "name": "Authorization"}}),
                )?;
            } else {
                map.serialize_entry("servers", &json!([{"url": base}]))?;
                map.serialize_entry("components", &json!({"securitySchemes": {"basic": {"type": "http", "scheme": "basic"}, "bearer": {"type": "http", "scheme": "bearer"}}}))?;
            }
            map.serialize_entry("paths", &Paths(self))?;
        }
        map.end()
    }
}

struct Paths<'a>(&'a Document<'a>);
impl Serialize for Paths<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.index.paths.len()))?;
        for (path, entries) in &self.0.index.paths {
            map.serialize_entry(path, &PathItem { document: self.0, entries, custom: false })?;
        }
        map.end()
    }
}

struct PathItem<'a> {
    document: &'a Document<'a>,
    entries: &'a [usize],
    custom: bool,
}
impl Serialize for PathItem<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        let mut has_custom = false;
        for index in self.entries {
            let entry = &self.document.index.requests[*index];
            let standard =
                matches!(entry.method.as_str(), "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace" | "query");
            has_custom |= !standard;
            if standard == self.custom {
                continue;
            }
            self.document.progress.check().map_err(S::Error::custom)?;
            let request: RequestFile = read(&entry.file).map_err(S::Error::custom)?;
            let operation = operation::operation(&request, entry, self.document.format).map_err(S::Error::custom)?;
            map.serialize_entry(&entry.method, &operation)?;
            self.document.progress.advance(1).map_err(S::Error::custom)?;
        }
        if has_custom && !self.custom {
            map.serialize_entry("additionalOperations", &PathItem { custom: true, ..*self })?;
        }
        map.end()
    }
}

struct NativeRoots<'a>(&'a Document<'a>);
impl Serialize for NativeRoots<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(1))?;
        sequence.serialize_element(&Folder { document: self.0, path: Vec::new() })?;
        sequence.end()
    }
}

struct Folder<'a> {
    document: &'a Document<'a>,
    path: Vec<String>,
}
impl Serialize for Folder<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("name", self.path.last().map(String::as_str).unwrap_or(""))?;
        if let Some(path) = self.document.index.folders.get(&self.path)
            && path.is_file()
        {
            let folder: volley_core::format::FolderFile = read(path).map_err(S::Error::custom)?;
            if let Some(docs) = folder.docs {
                map.serialize_entry("description", docs.content())?;
            }
        }
        map.serialize_entry("items", &Items(self))?;
        map.end()
    }
}

struct Items<'a>(&'a Folder<'a>);
impl Serialize for Items<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let node = &self.0.document.index.tree[&self.0.path];
        let mut sequence = serializer.serialize_seq(Some(node.requests.len() + node.children.len()))?;
        for index in &node.requests {
            let entry = &self.0.document.index.requests[*index];
            self.0.document.progress.check().map_err(S::Error::custom)?;
            let request: RequestFile = read(&entry.file).map_err(S::Error::custom)?;
            let value = native::request(&request, entry).map_err(S::Error::custom)?;
            sequence.serialize_element(&value)?;
            self.0.document.progress.advance(1).map_err(S::Error::custom)?;
        }
        for child in &node.children {
            let mut path = self.0.path.clone();
            path.push(child.clone());
            sequence.serialize_element(&Folder { document: self.0.document, path })?;
        }
        sequence.end()
    }
}
