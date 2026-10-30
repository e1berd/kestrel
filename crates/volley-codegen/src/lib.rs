mod build;
mod collections;
mod naming;
mod plugins;
mod publishing;
mod route;
mod security;
mod services;
mod typescript;
mod writing;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;
use volley_core::Collection;
use volley_core::transfer::Progress;

pub use collections::CollectionOptions;
pub use plugins::{CodegenPlugin, Generator};
pub use security::Authentication;
pub use writing::generate_to;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutputOptions {
    pub single_file: bool,
    pub archive: bool,
}

#[derive(Debug, Clone)]
pub struct Generated {
    pub files: BTreeMap<PathBuf, String>,
    pub entrypoint: PathBuf,
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
    pub collection: Option<CollectionOptions>,
    pub transport: Transport,
}

#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    pub name: String,
    pub wire: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transport {
    pub id: String,
    pub service: String,
    pub method: String,
    pub origin: String,
    pub path: String,
    pub path_parameters: Vec<Parameter>,
    pub query: Vec<(String, String)>,
    pub headers: BTreeMap<String, String>,
    pub sensitive_headers: Vec<String>,
    pub sensitive_query: Vec<String>,
    pub body_kind: String,
    pub auth: Option<Authentication>,
}

pub fn resources(collection: &Collection, progress: &Progress) -> Result<(Resource, Vec<String>)> {
    build::resources(collection, progress)
}

pub fn generate(collection: &Collection, progress: &Progress) -> Result<Generated> {
    Generator::new().generate(collection, progress)
}
mod archive;
