use std::collections::BTreeMap;

use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestPath {
    #[schemars(description = "Path to a saved request, relative to the collection root")]
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    #[serde(default)]
    #[schemars(description = "Existing collection folder; empty means the collection root")]
    pub folder: String,
    pub name: String,
    #[schemars(description = "HTTP method, for example GET or POST")]
    pub method: String,
    #[schemars(description = "URL, optionally containing {{variables}}")]
    pub url: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateRequest {
    pub path: String,
    #[schemars(
        description = "Partial OpenCollection request object. Objects merge recursively; arrays replace; null removes a field. Omitted fields and unknown keys are preserved. Example: {\"http\":{\"url\":\"{{baseUrl}}/users\"}}"
    )]
    pub patch: serde_json::Map<String, Value>,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Execute {
    #[serde(default)]
    #[schemars(description = "Relative request or folder path; empty runs the entire collection")]
    pub path: String,
    pub environment: Option<String>,
    #[serde(default)]
    #[schemars(description = "Runtime variables to set in this server session before execution")]
    pub variables: BTreeMap<String, Value>,
    #[serde(default)]
    pub bail: bool,
    #[schemars(description = "Maximum executions, including script redirects; 1..1000, default 100")]
    pub max_requests: Option<usize>,
    #[schemars(description = "Per-request timeout in seconds; 1..300, default 60")]
    pub timeout_seconds: Option<u64>,
}
