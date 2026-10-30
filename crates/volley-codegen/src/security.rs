use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Serialize;
use volley_core::Plan;
use volley_core::format::Auth;

#[derive(Debug, Clone, Serialize)]
pub struct Authentication {
    pub kind: String,
    pub key: String,
    pub placement: String,
    pub prefix: String,
    pub token: String,
    pub username: String,
    pub password: String,
}

pub(crate) fn authentication(plan: &Plan, warnings: &mut Vec<String>) -> Result<Option<Authentication>> {
    let auth = plan.auth();
    if matches!(auth.mode(), "none" | "inherit") {
        return Ok(None);
    }
    let Auth::Config(config) = auth else {
        bail!("authentication requires a configuration");
    };
    let kind = config.kind.as_str();
    if !matches!(kind, "bearer" | "basic" | "apikey" | "api-key" | "oauth2") {
        bail!("unsupported SDK authentication: {kind}");
    }
    if kind == "oauth2" {
        warnings.push("OAuth2 login is managed by the SDK consumer; supply an access token through token.".into());
    }
    let key = config.str("key");
    if matches!(kind, "apikey" | "api-key") && key.is_empty() {
        bail!("API key authentication has no key name");
    }
    let placement = if config.str("placement") == "query" || config.str("addTo") == "queryParams" { "query" } else { "header" };
    Ok(Some(Authentication {
        kind: kind.into(),
        key,
        placement: placement.into(),
        prefix: if config.str("prefix").is_empty() { "Bearer".into() } else { config.str("prefix") },
        token: template(&config.str(if kind == "apikey" || kind == "api-key" { "value" } else { "token" })),
        username: template(&config.str("username")),
        password: template(&config.str("password")),
    }))
}

fn template(value: &str) -> String {
    if !volley_core::vars::names(value).is_empty() { value.into() } else { String::new() }
}

pub(crate) fn sensitive(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace(['-', '_'], "");
    matches!(name.as_str(), "authorization" | "proxyauthorization" | "cookie" | "setcookie")
        || ["token", "secret", "password", "apikey"].iter().any(|part| name.contains(part))
}

pub(crate) fn headers(plan: &Plan) -> BTreeMap<String, String> {
    let http = plan.file.http.as_ref().expect("an HTTP request");
    let auth = plan.auth();
    let auth_header = auth.config().map(|config| match config.kind.as_str() {
        "bearer" | "basic" | "oauth2" => "authorization".to_string(),
        "apikey" | "api-key" if config.str("placement") != "query" && config.str("addTo") != "queryParams" => {
            config.str("key").to_ascii_lowercase()
        }
        _ => String::new(),
    });
    plan.chain
        .iter()
        .flat_map(|(_, defaults)| &defaults.headers)
        .chain(&http.headers)
        .filter(|header| header.enabled())
        .filter(|header| !header.name.eq_ignore_ascii_case("cookie") && !header.name.eq_ignore_ascii_case("set-cookie"))
        .filter(|header| auth_header.as_ref().is_none_or(|name| !header.name.eq_ignore_ascii_case(name)))
        .map(|header| {
            let name =
                if volley_core::vars::names(&header.name).is_empty() { header.name.to_ascii_lowercase() } else { header.name.clone() };
            let value = if (header.is_secret() || sensitive(&name)) && volley_core::vars::names(&header.value).is_empty() {
                format!("{{{{headers.{name}}}}}")
            } else {
                header.value.clone()
            };
            (name, value)
        })
        .collect()
}
