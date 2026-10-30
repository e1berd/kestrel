use anyhow::{Context as _, Result, bail};
use kestrel_core::format::Body;
use kestrel_core::transfer::{Progress, Unit};
use kestrel_core::{Collection, Plan};

use crate::assignment::Candidate;
use crate::naming::validate;
use crate::{Operation, Resource, Transport, assignment, collections, route, security, services};

pub(crate) fn resources(collection: &Collection, progress: &Progress) -> Result<(Resource, Vec<String>)> {
    let requests = collection.requests();
    progress.start("Building SDK resources", Some(requests.len() as u64), Unit::Requests)?;
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    for request in requests {
        progress.check()?;
        if request.doc.data.http.is_none() {
            warnings.push(format!("Skipped non-HTTP request: {}", request.name()));
            progress.advance(1)?;
            continue;
        }
        let plan = Plan::with_file(collection, &request.doc.path, request.doc.data.clone(), None)?;
        candidates
            .push(operation(&plan, &mut warnings).with_context(|| format!("generate SDK method for {}", request.doc.path.display()))?);
        progress.advance(1)?;
    }
    if candidates.is_empty() {
        bail!("the collection has no HTTP requests to generate");
    }
    let root = assignment::resources(candidates, &mut warnings)?;
    warnings.sort();
    warnings.dedup();
    Ok((root, warnings))
}

fn operation(plan: &Plan, warnings: &mut Vec<String>) -> Result<Candidate> {
    let http = plan.file.http.as_ref().expect("an HTTP request");
    let route = route::parse(&http.url)?;
    let method = if http.method.is_empty() { "GET".into() } else { http.method.to_ascii_uppercase() };
    let is_collection = method == "GET" && !route.member && route.action.is_none();
    if !method.bytes().all(|byte| byte.is_ascii_uppercase() || byte == b'-') {
        bail!("invalid HTTP method");
    }
    if matches!(method.as_str(), "CONNECT" | "TRACE" | "TRACK") {
        bail!("fetch does not support the {method} method");
    }
    let overrides = plan.file.extra.get("extensions").and_then(|value| value.get("kestrel")).and_then(|value| value.get("codegen"));
    let service = match overrides.and_then(|value| value.get("service")) {
        Some(value) => value.as_str().context("codegen.service must be a service name")?.to_string(),
        None => services::name(&route.origin)?,
    };
    validate(&service)?;
    let resource_path = match overrides.and_then(|value| value.get("resource")) {
        Some(value) => value.as_str().context("codegen.resource must be a dotted resource path")?.split('.').map(str::to_string).collect(),
        None => route.resources,
    };
    let inferred = route.action.unwrap_or_else(|| {
        match (method.as_str(), route.member) {
            ("GET", false) => "list",
            ("GET", true) => "get",
            ("POST", _) => "insert",
            ("PATCH", _) => "update",
            ("PUT", _) => "replace",
            ("DELETE", _) => "delete",
            _ => return method.to_ascii_lowercase(),
        }
        .into()
    });
    let name = match overrides.and_then(|value| value.get("method")) {
        Some(value) => value.as_str().context("codegen.method must be a string")?.to_string(),
        None => inferred,
    };
    validate(&name)?;
    for resource in &resource_path {
        validate(resource)?;
    }
    if !plan.file.scripts().is_empty() || plan.chain.iter().any(|(_, defaults)| !defaults.scripts.is_empty()) {
        warnings.push("Bruno scripts and tests are not executed in generated SDKs; reproduce their behavior in your application.".into());
    }
    if plan
        .chain
        .iter()
        .flat_map(|(_, defaults)| &defaults.headers)
        .chain(&http.headers)
        .any(|header| header.enabled() && (header.name.eq_ignore_ascii_case("cookie") || header.name.eq_ignore_ascii_case("set-cookie")))
    {
        warnings
            .push("Saved Cookie headers are omitted; configure per-service credentials and Cookie Store for session/CSRF cookies.".into());
    }
    if !http.extra.is_empty() || plan.file.settings.as_ref().is_some_and(|settings| settings.timeout.is_some_and(|timeout| timeout > 0)) {
        warnings.push("Kestrel transport settings are not embedded; configure fetch and RequestOptions.signal in your application.".into());
    }
    let body_kind = http.body.as_ref().map(|body| body.kind.as_str()).unwrap_or("none");
    if matches!(method.as_str(), "GET" | "HEAD") && body_kind != "none" {
        bail!("fetch cannot send a {method} request body");
    }
    let body = body_type(http.body.as_ref())?;
    let mut query = route.query;
    for parameter in http.params.iter().filter(|parameter| parameter.enabled() && parameter.kind.as_deref() != Some("path")) {
        query.push((parameter.name.clone(), parameter.value.clone()));
    }
    for (name, value) in &mut query {
        let secret = security::sensitive(name) || http.params.iter().any(|parameter| parameter.name == *name && parameter.is_secret());
        if secret && kestrel_core::vars::names(value).is_empty() {
            *value = format!("{{{{query.{name}}}}}");
        }
    }
    let mut query_parameters: Vec<_> = query.iter().map(|(name, _)| name.clone()).collect();
    query_parameters.sort();
    query_parameters.dedup();
    let collection =
        collections::options(plan, is_collection, resource_path.last().context("an SDK resource must have a name")?, &query_parameters)?;
    let transport = Transport {
        id: format!("{}.{name}", resource_path.join(".")),
        service,
        method,
        origin: route.origin,
        path: route.path,
        path_parameters: route.parameters.clone(),
        query,
        headers: security::headers(plan),
        sensitive_headers: plan
            .chain
            .iter()
            .flat_map(|(_, defaults)| &defaults.headers)
            .chain(&http.headers)
            .filter(|header| header.enabled() && (header.is_secret() || security::sensitive(&header.name)))
            .map(|header| header.name.clone())
            .collect(),
        sensitive_query: http
            .params
            .iter()
            .filter(|parameter| parameter.enabled() && parameter.kind.as_deref() != Some("path") && parameter.is_secret())
            .map(|parameter| parameter.name.clone())
            .collect(),
        body_kind: body_kind.into(),
        auth: security::authentication(plan, warnings)?,
    };
    Ok(Candidate {
        resources: resource_path,
        name,
        explicit: overrides.and_then(|value| value.get("method")).is_some(),
        source: plan.path.clone(),
        operation: Operation {
            request: plan.name.clone(),
            path_parameters: route.parameters,
            query_parameters,
            body,
            collection,
            transport,
        },
    })
}

fn body_type(body: Option<&Body>) -> Result<Option<String>> {
    Ok(match body.map(|body| body.kind.as_str()) {
        None | Some("none") => None,
        Some("json") => Some("JsonValue | BodyInit".into()),
        Some("graphql") => Some("{ query: string; variables?: Record<string, JsonValue>; operationName?: string }".into()),
        Some("form-urlencoded") => Some("URLSearchParams | Record<string, string>".into()),
        Some("multipart-form") => Some("FormData".into()),
        Some("text" | "xml" | "sparql" | "raw") => Some("string".into()),
        Some(kind) => bail!("unsupported SDK request body: {kind}"),
    })
}
