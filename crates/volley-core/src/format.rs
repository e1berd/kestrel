//! The files of an OpenCollection (the YAML format of Bruno 3): `opencollection.yml` at the root,
//! `folder.yml` in each folder, one file per request and `environments/*.yml`.
//!
//! Every type keeps the keys it does not know in `extra`, so saving never drops what another tool
//! wrote.

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml_ng::Value;

pub type Extra = IndexMap<String, Value>;

fn is_false(b: &bool) -> bool {
    !*b
}

/// Strings that YAML may have read as numbers or booleans (`value: 5`).
fn scalar_string<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(de)? {
        Value::Null => String::new(),
        Value::String(s) => s,
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_yaml_ng::to_string(&other)
            .unwrap_or_default()
            .trim_end()
            .to_string(),
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Info {
    #[serde(default, deserialize_with = "scalar_string")]
    pub name: String,
    #[serde(rename = "type", default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A header, a parameter or a variable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KeyValue {
    #[serde(default, deserialize_with = "scalar_string")]
    pub name: String,
    #[serde(default, deserialize_with = "scalar_string")]
    pub value: String,
    /// `path` or `query` for parameters; `text` or `file` for multipart fields.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl KeyValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Default::default()
        }
    }

    pub fn with_kind(mut self, kind: &str) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn enabled(&self) -> bool {
        !self.disabled
    }

    pub fn is_secret(&self) -> bool {
        self.extra
            .get("secret")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Body {
    /// `none`, `json`, `text`, `xml`, `sparql`, `form-urlencoded`, `multipart-form` or `graphql`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Body {
    pub fn text(kind: &str, data: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            data: Value::String(data.into()),
            extra: Extra::new(),
        }
    }

    /// The text of a json, text, xml or sparql body.
    pub fn as_text(&self) -> String {
        match &self.data {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => serde_yaml_ng::to_string(other).unwrap_or_default(),
        }
    }

    /// The fields of a form body.
    pub fn fields(&self) -> Vec<KeyValue> {
        serde_yaml_ng::from_value(self.data.clone()).unwrap_or_default()
    }

    pub fn set_fields(&mut self, fields: &[KeyValue]) {
        self.data = serde_yaml_ng::to_value(fields).unwrap_or(Value::Null);
    }

    /// `query` and `variables` of a GraphQL body.
    pub fn graphql(&self) -> (String, String) {
        let get = |key: &str| {
            self.data
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        (get("query"), get("variables"))
    }

    pub fn is_form(&self) -> bool {
        matches!(self.kind.as_str(), "form-urlencoded" | "multipart-form")
    }
}

/// `auth: inherit`, `auth: none`, or the settings of one scheme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Auth {
    Mode(String),
    Config(AuthConfig),
}

impl Default for Auth {
    fn default() -> Self {
        Auth::Mode("inherit".into())
    }
}

impl Auth {
    /// `inherit`, `none`, `bearer`, `basic`, `apikey`, `oauth2`, …
    pub fn mode(&self) -> &str {
        match self {
            Auth::Mode(mode) => mode,
            Auth::Config(config) => &config.kind,
        }
    }

    pub fn config(&self) -> Option<&AuthConfig> {
        match self {
            Auth::Config(config) => Some(config),
            Auth::Mode(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AuthConfig {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub fields: Extra,
}

impl AuthConfig {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            fields: Extra::new(),
        }
    }

    /// A string at a dotted path such as `credentials.clientId`.
    pub fn str(&self, path: &str) -> String {
        let mut parts = path.split('.');
        let first = parts.next().unwrap_or_default();
        let mut value = self.fields.get(first);
        for part in parts {
            value = value.and_then(|v| v.get(part));
        }
        match value {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Bool(b)) => b.to_string(),
            _ => String::new(),
        }
    }

    pub fn bool(&self, path: &str) -> bool {
        self.str(path) == "true"
    }

    pub fn has(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.fields.insert(key.into(), Value::String(value.into()));
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Script {
    /// `before-request`, `after-response` or `tests`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub code: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Http {
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// What a folder or the collection sets for every request under it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<Script>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Runtime {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<Script>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<bool>,
    /// Milliseconds; 0 waits for as long as it takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            encode_url: Some(true),
            timeout: Some(0),
            follow_redirects: Some(true),
            max_redirects: Some(5),
            extra: Extra::new(),
        }
    }
}

/// Free text, or `{ content, type }` with its media type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Docs {
    Text(String),
    Rich {
        content: String,
        #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
}

impl Docs {
    pub fn content(&self) -> &str {
        match self {
            Docs::Text(text) => text,
            Docs::Rich { content, .. } => content,
        }
    }
}

/// One request file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RequestFile {
    #[serde(default)]
    pub info: Info,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<Http>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<Runtime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<Docs>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl RequestFile {
    pub fn new_http(name: &str, method: &str, url: &str) -> Self {
        Self {
            info: Info {
                name: name.into(),
                kind: "http".into(),
                ..Default::default()
            },
            http: Some(Http {
                method: method.into(),
                url: url.into(),
                auth: Some(Auth::default()),
                ..Default::default()
            }),
            settings: Some(Settings::default()),
            ..Default::default()
        }
    }

    pub fn http(&self) -> Http {
        self.http.clone().unwrap_or_default()
    }

    pub fn scripts(&self) -> &[Script] {
        self.runtime
            .as_ref()
            .map(|r| r.scripts.as_slice())
            .unwrap_or_default()
    }

    pub fn variables(&self) -> &[KeyValue] {
        self.runtime
            .as_ref()
            .map(|r| r.variables.as_slice())
            .unwrap_or_default()
    }

    pub fn settings(&self) -> Settings {
        self.settings.clone().unwrap_or_default()
    }
}

/// `folder.yml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FolderFile {
    #[serde(default)]
    pub info: Info,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Defaults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<Docs>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// `opencollection.yml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CollectionFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencollection: Option<String>,
    #[serde(default)]
    pub info: Info,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Defaults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<Docs>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl CollectionFile {
    /// The OpenAPI specs this collection is generated from, as `(source, group_by)`. Read from
    /// `extensions.volley.openapi` or, in a collection Bruno made, `extensions.bruno.openapi`.
    pub fn openapi_sources(&self) -> Vec<(String, String)> {
        let extensions = self.extra.get("extensions");
        ["volley", "bruno"]
            .iter()
            .filter_map(|tool| extensions?.get(tool)?.get("openapi")?.as_sequence())
            .flatten()
            .filter_map(|source| {
                let url = source.get("sourceUrl")?.as_str()?.to_string();
                let group_by = source
                    .get("groupBy")
                    .and_then(Value::as_str)
                    .unwrap_or("tags")
                    .to_string();
                Some((url, group_by))
            })
            .collect()
    }

    /// Folder and file names the collection asks to skip.
    pub fn ignored(&self) -> Vec<String> {
        let extensions = self.extra.get("extensions");
        ["volley", "bruno"]
            .iter()
            .filter_map(|tool| extensions?.get(tool)?.get("ignore")?.as_sequence())
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }
}

/// `environments/<name>.yml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentFile {
    #[serde(default, deserialize_with = "scalar_string")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<KeyValue>,
    #[serde(flatten)]
    pub extra: Extra,
}
