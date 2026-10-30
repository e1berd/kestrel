use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use kestrel_core::format::RequestFile;
use kestrel_core::{Collection, Item};
use serde_json::{Value, json};

use crate::parameters::{CreateRequest, UpdateRequest};

pub fn resolve(collection: &Collection, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    ensure!(
        !path.is_absolute() && !path.components().any(|part| matches!(part, Component::ParentDir)),
        "use a path relative to the collection without '..'"
    );
    let path = collection.root.join(path).canonicalize().with_context(|| format!("find `{relative}`"))?;
    ensure!(path.starts_with(&collection.root), "path leaves the configured collection");
    Ok(path)
}

pub fn relative(collection: &Collection, path: &Path) -> Result<String> {
    Ok(path.strip_prefix(&collection.root)?.to_string_lossy().replace('\\', "/"))
}

pub fn info(collection: Collection) -> Result<Value> {
    Ok(json!({
        "name": collection.name(),
        "requests": collection.requests().len(),
        "environments": collection.environments.iter().map(|environment| environment.name()).collect::<Vec<_>>(),
    }))
}

pub fn list(collection: Collection) -> Result<Value> {
    let requests = collection
        .requests()
        .into_iter()
        .map(|request| {
            resolve(&collection, &relative(&collection, &request.doc.path)?)?;
            Ok(json!({
                "path": relative(&collection, &request.doc.path)?,
                "name": request.name(),
                "method": request.method(),
                "url": request.doc.data.http.as_ref().map(|http| &http.url),
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"requests": requests}))
}

pub fn get(collection: Collection, path: &str) -> Result<Value> {
    let path = resolve(&collection, path)?;
    let request = collection.request(&path).context("path is not a request")?;
    Ok(json!({"path": relative(&collection, &path)?, "request": request.doc.data}))
}

pub fn environments(collection: Collection) -> Result<Value> {
    for environment in &collection.environments {
        resolve(&collection, &relative(&collection, &environment.doc.path)?)?;
    }
    let environments = collection
        .environments
        .iter()
        .map(|environment| {
            json!({
                "name": environment.name(),
                "variables": environment.doc.data.variables.iter().map(|variable| json!({
                    "name": variable.name,
                    "value": if variable.is_secret() { Value::Null } else { json!(variable.value) },
                    "secret": variable.is_secret(),
                    "enabled": variable.enabled(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"environments": environments}))
}

pub fn create(mut collection: Collection, arguments: CreateRequest) -> Result<Value> {
    ensure!(!arguments.name.trim().is_empty(), "request name must not be empty");
    ensure!(
        kestrel_core::collection::sanitize_file_name(&arguments.name) != "folder",
        "folder.yml is reserved for folder metadata; choose another request name"
    );
    validate_method(&arguments.method)?;
    ensure!(!arguments.url.trim().is_empty(), "request URL must not be empty");
    let folder = resolve(&collection, &arguments.folder)?;
    ensure!(folder == collection.root || matches!(collection.item(&folder), Some(Item::Folder(_))), "path is not a collection folder");
    let file = RequestFile::new_http(&arguments.name, &arguments.method.to_uppercase(), &arguments.url);
    let path = collection.add_request(&folder, file)?;
    let path = relative(&collection, &path)?;
    get(collection, &path)
}

pub fn update(mut collection: Collection, arguments: UpdateRequest) -> Result<Value> {
    let path = resolve(&collection, &arguments.path)?;
    let request = collection.request_mut(&path).context("path is not a request")?;
    let mut data = serde_json::to_value(&request.doc.data)?;
    merge_patch(&mut data, Value::Object(arguments.patch));
    let data: RequestFile = serde_json::from_value(data)?;
    let http = data.http.as_ref().context("request must contain http")?;
    validate_method(&http.method)?;
    ensure!(!http.url.trim().is_empty(), "request URL must not be empty");
    ensure!(data.info.kind.is_empty() || data.info.kind == "http", "only HTTP requests are supported");
    if request.doc.data != data {
        request.doc.data = data;
        request.doc.save()?;
    }
    get(collection, &arguments.path)
}

fn validate_method(method: &str) -> Result<()> {
    ensure!(!method.is_empty(), "HTTP method must not be empty");
    if !method.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)) {
        bail!("invalid HTTP method");
    }
    Ok(())
}

fn merge_patch(target: &mut Value, patch: Value) {
    if let Value::Object(patch) = patch {
        if !target.is_object() {
            *target = json!({});
        }
        if let Value::Object(target) = target {
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(&key);
                } else {
                    merge_patch(target.entry(key).or_insert(Value::Null), value);
                }
            }
        }
    } else {
        *target = patch;
    }
}
