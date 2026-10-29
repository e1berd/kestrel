use anyhow::{Context as _, Result, bail};

use crate::Parameter;
use crate::naming::{identifier, validate};

pub(crate) struct Route {
    pub origin: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub parameters: Vec<Parameter>,
    pub resources: Vec<String>,
    pub action: Option<String>,
    pub member: bool,
}

pub(crate) fn parse(input: &str) -> Result<Route> {
    let input = input.trim();
    let input = input.split('#').next().unwrap_or(input);
    let (head, query) = input.split_once('?').unwrap_or((input, ""));
    let (origin, path) = if head.starts_with("{{") {
        let end = head.find("}}").context("unclosed base URL variable")? + 2;
        (&head[..end], &head[end..])
    } else if let Some(scheme) = head.find("://") {
        let end = head[scheme + 3..].find('/').map(|offset| scheme + 3 + offset).unwrap_or(head.len());
        let origin = &head[..end];
        let parsed = url::Url::parse(origin).context("parse the request origin")?;
        if !matches!(parsed.scheme(), "http" | "https") || !parsed.username().is_empty() || parsed.password().is_some() {
            bail!("SDK URLs must use HTTP(S) without embedded credentials");
        }
        (origin, &head[end..])
    } else if head.starts_with('/') {
        ("", head)
    } else {
        bail!("SDK URLs must be absolute, relative to /, or start with a base URL variable");
    };
    if !path.is_empty() && !path.starts_with('/') {
        bail!("the request path must start with /");
    }
    let mut parameters = Vec::new();
    let mut resources = Vec::new();
    let mut action = None;
    let mut member = false;
    let mut segments = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            segments.push(String::new());
            continue;
        }
        let (segment, custom) = if !segment.starts_with(':') {
            segment.split_once(':').map_or((segment, None), |(resource, action)| (resource, Some(action)))
        } else if let Some((parameter, action)) = segment[1..].split_once(':') {
            (&segment[..parameter.len() + 1], Some(action))
        } else {
            (segment, None)
        };
        let parameter = segment
            .strip_prefix(':')
            .or_else(|| segment.strip_prefix("{{").and_then(|value| value.strip_suffix("}}")))
            .or_else(|| segment.strip_prefix('{').and_then(|value| value.strip_suffix('}')));
        member = parameter.is_some();
        if let Some(wire) = parameter {
            if wire.is_empty() {
                bail!("empty path parameter");
            }
            let name = identifier(wire);
            validate(&name)?;
            if matches!(name.as_str(), "body" | "query") {
                bail!("path parameter `{wire}` conflicts with an SDK argument; rename it in the request");
            }
            if parameters.iter().any(|parameter: &Parameter| parameter.name == name && parameter.wire != wire) {
                bail!("path parameter names collide after normalization: {wire}");
            }
            if !parameters.iter().any(|parameter: &Parameter| parameter.wire == wire) {
                parameters.push(Parameter { name, wire: wire.into() });
            }
            segments.push(format!("{{{wire}}}"));
        } else {
            segments.push(segment.into());
            if !(resources.is_empty() && is_prefix(segment)) {
                resources.push(identifier(segment));
            }
        }
        if let Some(custom) = custom {
            if custom.is_empty() {
                bail!("empty custom method");
            }
            action = Some(identifier(custom));
            let last = segments.last_mut().expect("a route segment");
            last.push(':');
            last.push_str(custom);
        }
    }
    if resources.is_empty() {
        resources.push("root".into());
    }
    Ok(Route {
        origin: origin.into(),
        path: if path.is_empty() { "/".into() } else { segments.join("/") },
        query: url::form_urlencoded::parse(query.as_bytes()).map(|(name, value)| (name.into_owned(), value.into_owned())).collect(),
        parameters,
        resources,
        action,
        member,
    })
}

fn is_prefix(segment: &str) -> bool {
    segment == "api"
        || segment.strip_prefix('v').is_some_and(|version| !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.'))
}
