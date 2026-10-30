use anyhow::{Context as _, Result};
use kestrel_core::format::Docs;
use kestrel_openapi::GroupBy;
use serde_json::{Value as Json, json};

pub(crate) mod request;

pub(crate) fn docs(node: &Json) -> Option<Docs> {
    node.get("description")
        .and_then(Json::as_str)
        .filter(|description| !description.is_empty())
        .map(|description| Docs::Text(description.into()))
}

pub(crate) fn warn_processors(node: &Json, warnings: &mut Vec<String>) {
    if ["preProcessors", "postProcessors"].iter().any(|key| node.get(key).and_then(Json::as_array).is_some_and(|items| !items.is_empty())) {
        warnings.push("Apidog scripts, assertions and processors are not imported.".into());
    }
}

pub(super) fn generated_request(spec: &Json, path: &str, method: &str, operation: Json) -> Result<kestrel_core::format::RequestFile> {
    let mut spec = spec.clone();
    spec["paths"] = json!({path: {"additionalOperations": {method: operation}}});
    kestrel_openapi::generate(&spec, GroupBy::Tags)
        .operations
        .into_iter()
        .next()
        .map(|operation| operation.file)
        .context("could not generate the Apidog HTTP request")
}
