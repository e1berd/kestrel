use anyhow::{Context as _, Result, bail};
use kestrel_core::Plan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionOptions {
    pub resource_name: String,
    pub items: Option<String>,
    pub page_token: Option<String>,
    pub next_page_token: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Settings {
    items: Option<String>,
    page_token: Option<String>,
    next_page_token: Option<String>,
}

pub(crate) fn options(plan: &Plan, inferred: bool, resource_name: &str, query_parameters: &[String]) -> Result<Option<CollectionOptions>> {
    let configured = plan
        .file
        .extra
        .get("extensions")
        .and_then(|value| value.get("kestrel"))
        .and_then(|value| value.get("codegen"))
        .and_then(|value| value.get("collection"));
    let settings = match configured {
        None if !inferred => return Ok(None),
        None => Settings::default(),
        Some(value) if value.as_bool() == Some(false) => return Ok(None),
        Some(value) if value.as_bool() == Some(true) => Settings::default(),
        Some(value) => serde_json::from_value::<Settings>(serde_json::to_value(value)?)
            .context("codegen.collection must be a boolean or an object with items, pageToken and nextPageToken")?,
    };
    for (name, path) in [("items", settings.items.as_deref()), ("nextPageToken", settings.next_page_token.as_deref())] {
        if let Some(path) = path
            && !(name == "items" && path.is_empty())
            && path.split('.').any(str::is_empty)
        {
            bail!("codegen.collection.{name} must be a dotted response field path");
        }
    }
    if settings.page_token.as_ref().is_some_and(|name| name.trim().is_empty()) {
        bail!("codegen.collection.pageToken must be a nonempty query parameter name");
    }
    let page_token =
        settings.page_token.or_else(|| query_parameters.iter().find(|name| *name == "pageToken" || *name == "page_token").cloned());
    Ok(Some(CollectionOptions {
        resource_name: resource_name.into(),
        items: settings.items,
        page_token,
        next_page_token: settings.next_page_token,
    }))
}
