use std::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};

use crate::Resource;
use crate::naming::{identifier, validate};

pub(crate) fn name(origin: &str) -> Result<String> {
    if origin.is_empty() {
        return Ok("default".into());
    }
    if let Some(variable) = origin.strip_prefix("{{").and_then(|value| value.strip_suffix("}}")) {
        return Ok(if variable.trim() == "baseUrl" { "default".into() } else { identifier(variable.trim()) });
    }
    let url = url::Url::parse(origin)?;
    let mut name = identifier(url.host_str().context("a service URL must have a host")?);
    if let Some(port) = url.port() {
        name.push_str(&format!("Port{port}"));
    }
    Ok(name)
}

pub(crate) fn synchronize(root: &mut Resource, path: &[String], services: &mut BTreeMap<String, String>) -> Result<()> {
    for (name, resource) in &mut root.resources {
        validate(name)?;
        if root.methods.contains_key(name) {
            bail!("SDK resource `{name}` collides with a method");
        }
        let mut path = path.to_vec();
        path.push(name.clone());
        synchronize(resource, &path, services)?;
    }
    for (name, operation) in &mut root.methods {
        validate(name)?;
        validate(&operation.transport.service)?;
        operation.transport.id = format!("{}.{name}", path.join("."));
        let origin = canonical_origin(&operation.transport.origin)?;
        if let Some(previous) = services.insert(operation.transport.service.clone(), origin.clone())
            && previous != origin
        {
            bail!(
                "service `{}` refers to several origins; set distinct extensions.kestrel.codegen.service names",
                operation.transport.service
            );
        }
    }
    Ok(())
}

fn canonical_origin(origin: &str) -> Result<String> {
    if origin.is_empty() {
        return Ok("{{baseUrl}}".into());
    }
    if let Some(variable) = origin.strip_prefix("{{").and_then(|value| value.strip_suffix("}}")) {
        return Ok(format!("{{{{{}}}}}", variable.trim()));
    }
    let url = url::Url::parse(origin)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("service origins must be HTTP(S) origins without credentials, path, query or fragment");
    }
    Ok(url.origin().ascii_serialization())
}
