mod aggregate;
mod bundle;
mod modules;
mod operation;
mod query;
mod rstore;
mod runtime;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use kestrel_core::transfer::Progress;

use crate::{Operation, OutputOptions, Resource};

pub(crate) type Files = BTreeMap<PathBuf, String>;

pub(crate) fn generate(
    root: &Resource,
    services: &BTreeMap<String, String>,
    plugins: &[(&str, &str, &str)],
    options: OutputOptions,
    progress: &Progress,
) -> Result<(Files, Vec<String>, Vec<String>)> {
    if options.single_file {
        return bundle::generate(root, services, plugins, options.data_layer, progress);
    }
    modules::generate(root, services, plugins, options.data_layer, progress)
}

fn insert(files: &mut Files, path: &Path, javascript: String, declarations: String) {
    files.insert(path.to_path_buf(), javascript);
    files.insert(path.with_extension("d.ts"), declarations);
}

fn register(root: &Resource, path: &[String], names: &mut BTreeMap<Vec<String>, String>, used: &mut BTreeSet<String>) {
    let base = if path.is_empty() {
        "Client".into()
    } else {
        let parts: String = path
            .iter()
            .map(|part| {
                let mut part = part.clone();
                part.get_mut(..1).expect("an ASCII resource name").make_ascii_uppercase();
                part
            })
            .collect();
        format!("{parts}Resource")
    };
    let mut name = base.clone();
    let mut suffix = 2;
    while !used.insert(name.to_ascii_lowercase()) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    names.insert(path.to_vec(), name);
    for (name, child) in &root.resources {
        let mut path = path.to_vec();
        path.push(name.clone());
        register(child, &path, names, used);
    }
}

fn parameters(operation: &Operation) -> Result<String> {
    let mut fields = Vec::new();
    for parameter in &operation.path_parameters {
        fields.push(format!("{}: string | number", serde_json::to_string(&parameter.name)?));
    }
    let mut query = "Record<string, QueryValue>".to_string();
    if !operation.query_parameters.is_empty() {
        let names = operation
            .query_parameters
            .iter()
            .map(|name| Ok(format!("{}?: QueryValue", serde_json::to_string(name)?)))
            .collect::<Result<Vec<_>>>()?;
        query.push_str(&format!(" & {{ {} }}", names.join("; ")));
    }
    fields.push(format!("query?: {query}"));
    if let Some(body) = &operation.body {
        fields.push(format!("body: {body}"));
    }
    fields.push("search?: SearchParams".into());
    Ok(format!("{{ {} }}", fields.join("; ")))
}
