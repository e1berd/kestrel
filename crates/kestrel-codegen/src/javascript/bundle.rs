use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;
use kestrel_core::transfer::Progress;

use crate::Resource;

use super::{Files, insert};

pub(super) fn generate(
    root: &Resource,
    services: &BTreeMap<String, String>,
    plugins: &[(&str, &str, &str)],
    tanstack_query: bool,
    progress: &Progress,
) -> Result<(Files, Vec<String>)> {
    let mut javascript = String::new();
    let mut declarations = String::new();
    let templates = super::runtime::files(tanstack_query);
    for name in ["web", "runtime", "routing", "auth", "cookies", "hooks", "executor", "collection", "tanstack"] {
        let path = format!("lib/{name}.js");
        if let Some(source) = templates.get(Path::new(&path)) {
            javascript.push_str(&without_imports(source).replace(&format!("./{name}.js"), "./client.js"));
            let types = Path::new(&path).with_extension("d.ts");
            declarations.push_str(&without_imports(&templates[&types]));
        }
    }
    let service_types = services
        .iter()
        .map(|(name, origin)| Ok(format!("readonly {}: {}", serde_json::to_string(name)?, serde_json::to_string(origin)?)))
        .collect::<Result<Vec<_>>>()?
        .join("; ");
    javascript.push_str(&format!("\nexport const services = Object.freeze({});\n", serde_json::to_string(services)?));
    declarations
        .push_str(&format!("\nexport declare const services: {{ {service_types} }};\nexport type ServiceName = keyof typeof services;\n"));
    javascript.push_str("\nexport const operations = Object.freeze({\n");
    declarations.push_str("export declare const operations: Readonly<{\n");
    descriptors(root, &mut javascript, &mut declarations, progress)?;
    declarations.push_str("}>;\nexport type OperationId = keyof typeof operations;\n");
    javascript.push_str("});\n");
    let plugin_names: Vec<_> = plugins.iter().map(|(name, _, _)| name).collect();
    javascript.push_str(&format!(
        "\nexport const generation = Object.freeze({{ version: 1, plugins: Object.freeze({}) }});\n",
        serde_json::to_string(&plugin_names)?
    ));
    declarations.push_str("export declare const generation: { readonly version: 1; readonly plugins: readonly string[] };\n");
    for (_, runtime, types) in plugins {
        javascript.push_str(runtime);
        javascript.push('\n');
        declarations.push_str(types);
        declarations.push('\n');
    }
    let mut names = BTreeMap::new();
    super::register(root, &[], &mut names, &mut BTreeSet::new());
    let mut methods = Vec::new();
    Renderer { names: &names, javascript: &mut javascript, declarations: &mut declarations, methods: &mut methods, tanstack_query }
        .render(root, &[], progress)?;
    javascript.push_str("\n/** @param {import('./client.js').ClientOptions} [options] */\nexport function createClient(options = {}) {\n  return new Client(options);\n}\n");
    declarations.push_str("export declare function createClient(options?: ClientOptions): Client;\n");
    let mut files = Files::new();
    insert(&mut files, Path::new("client.js"), javascript, declarations);
    Ok((files, methods))
}

fn without_imports(source: &str) -> String {
    source.lines().filter(|line| !line.starts_with("import ") && *line != "export {};").map(|line| format!("{line}\n")).collect()
}

fn descriptors(root: &Resource, source: &mut String, declarations: &mut String, progress: &Progress) -> Result<()> {
    for child in root.resources.values() {
        descriptors(child, source, declarations, progress)?;
    }
    for operation in root.methods.values() {
        progress.check()?;
        declarations.push_str(&format!("  {}: Operation;\n", serde_json::to_string(&operation.transport.id)?));
        source.push_str(&format!(
            "  {}: defineOperation({}),\n",
            serde_json::to_string(&operation.transport.id)?,
            serde_json::to_string(&operation.transport)?
        ));
    }
    Ok(())
}

struct Renderer<'a> {
    names: &'a BTreeMap<Vec<String>, String>,
    javascript: &'a mut String,
    declarations: &'a mut String,
    methods: &'a mut Vec<String>,
    tanstack_query: bool,
}

impl Renderer<'_> {
    fn render(&mut self, root: &Resource, path: &[String], progress: &Progress) -> Result<()> {
        for (name, child) in &root.resources {
            let mut child_path = path.to_vec();
            child_path.push(name.clone());
            self.render(child, &child_path, progress)?;
        }
        progress.check()?;
        let Self { names, javascript, declarations, methods, tanstack_query } = self;
        let tanstack_query = *tanstack_query;
        let class = &names[path];
        javascript.push_str(&format!("\nexport class {class} extends Resource {{\n"));
        let mut class_types = format!("\nexport declare class {class} extends Resource {{\n");
        if path.is_empty() {
            javascript.push_str("  /** @param {import('./client.js').ClientOptions} [options] */\n  constructor(options = {}) {\n    super(createExecutor(options));\n  }\n");
            class_types.push_str("  constructor(options?: ClientOptions);\n");
        }
        for name in root.resources.keys() {
            let mut child_path = path.to_vec();
            child_path.push(name.clone());
            let child_class = &names[&child_path];
            javascript.push_str(&format!("  {name} = new {child_class}(this[resourceExecutor]);\n"));
            class_types.push_str(&format!("  readonly {name}: {child_class};\n"));
        }
        for (name, operation) in &root.methods {
            progress.check()?;
            let callable = format!("Operation{}", methods.len());
            declarations.push_str(&super::operation::callable(operation, &callable, "Operation", tanstack_query.then_some(""))?);
            class_types.push_str(&format!("  readonly {name}: {callable};\n"));
            let default = if operation.path_parameters.is_empty() && operation.body.is_none() { " = {}" } else { "" };
            let descriptor = format!("operations[{}]", serde_json::to_string(&operation.transport.id)?);
            let invoke = if let Some(collection) = &operation.collection {
                format!("createCollection(this[resourceExecutor], {descriptor}, params, options, {})", serde_json::to_string(collection)?)
            } else {
                format!("this[resourceExecutor]({descriptor}, params, options)")
            };
            javascript.push_str(&format!("  /** @type {{import('./client.js').{class}['{name}']}} */\n  {name} = "));
            if tanstack_query {
                javascript.push_str("withQueryOptions(");
            }
            javascript.push_str(&format!(
            "Object.assign((params{default}, options = {{}}) =>\n    {invoke}, {{\n    operation: {descriptor},\n    request: (params{default}, options = {{}}) => this[resourceExecutor].request({descriptor}, params, options),\n    response: (params{default}, options = {{}}) => this[resourceExecutor].response({descriptor}, params, options),\n  }})"
        ));
            if tanstack_query {
                javascript.push_str(", this[resourceExecutor])");
            }
            javascript.push_str(";\n");
            methods.push(operation.transport.id.clone());
        }
        javascript.push_str("}\n");
        class_types.push_str("}\n");
        declarations.push_str(&class_types);
        Ok(())
    }
}
