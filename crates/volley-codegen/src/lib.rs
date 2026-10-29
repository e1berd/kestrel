mod build;
mod naming;
mod route;
mod security;
mod typescript;
mod writing;

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use volley_core::Collection;
use volley_core::transfer::{Progress, Unit};

pub use writing::generate_file;

#[derive(Debug, Clone)]
pub struct Generated {
    pub source: String,
    pub requests: usize,
    pub methods: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Resource {
    pub resources: BTreeMap<String, Resource>,
    pub methods: BTreeMap<String, Operation>,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub request: String,
    pub path_parameters: Vec<Parameter>,
    pub query_parameters: Vec<String>,
    pub body: Option<String>,
    pub(crate) transport: Transport,
}

#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    pub name: String,
    pub wire: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Transport {
    method: String,
    origin: String,
    path: String,
    path_parameters: Vec<Parameter>,
    query: Vec<(String, String)>,
    headers: BTreeMap<String, String>,
    body_kind: String,
    auth: Option<security::Authentication>,
}

pub fn resources(collection: &Collection, progress: &Progress) -> Result<(Resource, Vec<String>)> {
    build::resources(collection, progress)
}

pub fn generate(collection: &Collection, progress: &Progress) -> Result<Generated> {
    let (resources, warnings) = resources(collection, progress)?;
    progress.start("Generating TypeScript client", None, Unit::Requests)?;
    let (source, methods) = typescript::generate(&resources, progress)?;
    Ok(Generated { source, requests: methods.len(), methods, warnings })
}
