use std::collections::BTreeSet;

use anyhow::Result;

use crate::{Operation, Resource};

pub(super) struct Rstore {
    pub(super) javascript: String,
    pub(super) declarations: String,
    pub(super) skipped: Vec<String>,
}

struct Collection<'a> {
    path: Vec<String>,
    name: String,
    list: Option<&'a Operation>,
    get: Option<&'a Operation>,
    insert: Option<&'a Operation>,
    update: Option<(&'static str, &'a Operation)>,
    delete: Option<&'a Operation>,
}

pub(super) fn generate(root: &Resource) -> Result<Rstore> {
    let mut collections = Vec::new();
    let mut skipped = Vec::new();
    let mut names = BTreeSet::new();
    collect(root, &mut Vec::new(), &mut names, &mut collections, &mut skipped);
    let mut javascript = String::from(
        "\nconst collectItems = async (items) => {\n  const result = [];\n  for await (const item of items) result.push(item);\n  return result;\n};\n\nconst nullWhenMissing = (error) => {\n  if (error instanceof ApiError && error.status === 404) return null;\n  throw error;\n};\n\nexport function createRstoreCollections(client) {\n  return [\n",
    );
    let mut declarations = String::from(
        "import type { Collection as RstoreCollection } from '@rstore/vue';\n\nexport interface RstoreItem {\n  id: string | number;\n  [field: string]: unknown;\n}\n",
    );
    let mut tuple = Vec::new();
    for collection in &collections {
        javascript.push_str(&render_collection(collection)?);
        let type_name = type_name(&collection.name);
        declarations.push_str(&format!(
            "export interface {type_name}Item extends RstoreItem {{}}\nexport type {type_name}Collection = RstoreCollection<{type_name}Item> & {{ readonly name: {}; readonly '~type': undefined; readonly '~item': {type_name}Item }};\n",
            serde_json::to_string(&collection.name)?
        ));
        tuple.push(format!("{type_name}Collection"));
    }
    javascript.push_str("  ];\n}\n");
    declarations.push_str(&format!("export declare function createRstoreCollections(client: Client): [{}];\n", tuple.join(", ")));
    Ok(Rstore { javascript, declarations, skipped })
}

fn collect<'a>(
    resource: &'a Resource,
    path: &mut Vec<String>,
    names: &mut BTreeSet<String>,
    collections: &mut Vec<Collection<'a>>,
    skipped: &mut Vec<String>,
) {
    for (name, child) in &resource.resources {
        path.push(name.clone());
        collect(child, path, names, collections, skipped);
        path.pop();
    }
    if path.is_empty() {
        return;
    }
    let method = |name: &str| resource.methods.get(name);
    let candidate = Collection {
        path: path.clone(),
        name: unique_name(path, names),
        list: method("list"),
        get: method("get"),
        insert: method("insert"),
        update: method("update")
            .map(|operation| ("update", operation))
            .or_else(|| method("replace").map(|operation| ("replace", operation))),
        delete: method("delete"),
    };
    if candidate.list.is_none() && candidate.get.is_none() {
        return;
    }
    if fits_one_key(&candidate) {
        collections.push(candidate);
    } else {
        skipped.push(format!("rstore: skipped `{}`: member routes need exactly one path parameter and list/create none", path.join(".")));
    }
}

fn fits_one_key(collection: &Collection) -> bool {
    let keyed = [collection.get, collection.update.map(|(_, operation)| operation), collection.delete]
        .into_iter()
        .flatten()
        .all(|operation| operation.path_parameters.len() == 1);
    let unkeyed = [collection.list, collection.insert].into_iter().flatten().all(|operation| operation.path_parameters.is_empty());
    keyed && unkeyed
}

fn unique_name(path: &[String], names: &mut BTreeSet<String>) -> String {
    let base: String = path.iter().enumerate().map(|(index, part)| if index == 0 { part.clone() } else { type_name(part) }).collect();
    let mut name = base.clone();
    let mut suffix = 2;
    while !names.insert(name.to_ascii_lowercase()) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    name
}

fn type_name(name: &str) -> String {
    let mut name = name.to_string();
    if let Some(first) = name.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    name
}

fn render_collection(collection: &Collection) -> Result<String> {
    let accessor: String = collection.path.iter().map(|part| format!(".{part}")).collect();
    let target = format!("client{accessor}");
    let mut hooks = Vec::new();
    if let Some(get) = collection.get {
        let key = serde_json::to_string(&get.path_parameters[0].name)?;
        hooks.push(format!("fetchFirst: ({{ key }}) => {target}.get({{ {key}: key }}).catch(nullWhenMissing)"));
    }
    if collection.list.is_some() {
        hooks.push(format!("fetchMany: ({{ params }}) => collectItems({target}.list({{ query: params }}))"));
    }
    if let Some(insert) = collection.insert {
        let body = if insert.body.is_some() { "{ body: item }" } else { "{}" };
        hooks.push(format!("create: ({{ item }}) => {target}.insert({body})"));
    }
    if let Some((method, update)) = collection.update {
        let key = serde_json::to_string(&update.path_parameters[0].name)?;
        let body = if update.body.is_some() { ", body: item" } else { "" };
        hooks.push(format!("update: ({{ key, item }}) => {target}.{method}({{ {key}: key{body} }})"));
    }
    if let Some(delete) = collection.delete {
        let key = serde_json::to_string(&delete.path_parameters[0].name)?;
        hooks.push(format!("delete: async ({{ key }}) => {{\n          await {target}.delete({{ {key}: key }});\n        }}"));
    }
    Ok(format!(
        "    withItemType().defineCollection({{\n      name: {},\n      hooks: {{\n        {},\n      }},\n    }}),\n",
        serde_json::to_string(&collection.name)?,
        hooks.join(",\n        ")
    ))
}
