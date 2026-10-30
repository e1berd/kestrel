mod aggregate;
mod modules;
mod operation;
mod runtime;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::Result;
use volley_core::transfer::Progress;

use crate::{Operation, Resource};

pub(crate) fn generate(
    root: &Resource,
    services: &BTreeMap<String, String>,
    plugins: &[(&str, &str)],
    single_file: bool,
    progress: &Progress,
) -> Result<(BTreeMap<PathBuf, String>, Vec<String>)> {
    if !single_file {
        return modules::generate(root, services, plugins, progress);
    }
    let mut source = [
        include_str!("../web.ts"),
        include_str!("../runtime.ts"),
        include_str!("../routing.ts"),
        include_str!("../auth.ts"),
        include_str!("../cookies.ts"),
        include_str!("../hooks.ts"),
        include_str!("../executor.ts"),
        include_str!("../collection.ts"),
    ]
    .join("\n");
    source.push_str(&format!(
        "\nexport const services = Object.freeze({} as const);\nexport type ServiceName = keyof typeof services;\n",
        serde_json::to_string(services)?
    ));
    source.push_str("\nexport const operations = Object.freeze({\n");
    descriptors(root, &mut source, progress)?;
    source.push_str("});\nexport type OperationId = keyof typeof operations;\n");
    let plugin_names: Vec<_> = plugins.iter().map(|(name, _)| name).collect();
    source.push_str(&format!(
        "\nexport const generation = Object.freeze({{ version: 1, plugins: Object.freeze({}) }});\n",
        serde_json::to_string(&plugin_names)?
    ));
    for (_, runtime) in plugins {
        source.push_str(runtime);
        source.push('\n');
    }
    let mut names = BTreeMap::new();
    register(root, &[], &mut names, &mut BTreeSet::new());
    let mut methods = Vec::new();
    render(root, &[], &names, &mut source, &mut methods, progress)?;
    source.push_str("\nexport function createClient(options: ClientOptions = {}): Client {\n  return new Client(options);\n}\n");
    Ok((BTreeMap::from([(PathBuf::from("client.ts"), source)]), methods))
}

fn descriptors(root: &Resource, source: &mut String, progress: &Progress) -> Result<()> {
    for child in root.resources.values() {
        descriptors(child, source, progress)?;
    }
    for operation in root.methods.values() {
        progress.check()?;
        source.push_str(&format!(
            "  {}: defineOperation({}),\n",
            serde_json::to_string(&operation.transport.id)?,
            serde_json::to_string(&operation.transport)?
        ));
    }
    Ok(())
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

fn render(
    root: &Resource,
    path: &[String],
    names: &BTreeMap<Vec<String>, String>,
    source: &mut String,
    methods: &mut Vec<String>,
    progress: &Progress,
) -> Result<()> {
    for (name, child) in &root.resources {
        let mut child_path = path.to_vec();
        child_path.push(name.clone());
        render(child, &child_path, names, source, methods, progress)?;
    }
    progress.check()?;
    source.push_str(&format!("\nexport class {} extends Resource {{\n", names[path]));
    if path.is_empty() {
        source.push_str("  constructor(options: ClientOptions = {}) {\n    super(createExecutor(options));\n  }\n");
    }
    for name in root.resources.keys() {
        let mut child_path = path.to_vec();
        child_path.push(name.clone());
        source.push_str(&format!("  readonly {name} = new {}(this[resourceExecutor]);\n", names[&child_path]));
    }
    for (name, operation) in &root.methods {
        progress.check()?;
        let parameters = parameters(operation)?;
        let default = if operation.path_parameters.is_empty() && operation.body.is_none() { " = {}" } else { "" };
        let descriptor = format!("operations[{}]", serde_json::to_string(&operation.transport.id)?);
        if let Some(collection) = &operation.collection {
            let collection = serde_json::to_string(collection)?;
            source.push_str(&format!(
                "  {name} = Object.assign(<Item = unknown>(params: {parameters}{default}, options: RequestOptions = {{}}): Collection<Item> =>\n    createCollection<Item>(this[resourceExecutor], {descriptor}, params, options, {collection}), {{\n"
            ));
        } else {
            source.push_str(&format!(
                "  {name} = Object.assign(<Response = unknown>(params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Response> =>\n    this[resourceExecutor]<Response>({descriptor}, params, options), {{\n"
            ));
        }
        source.push_str(&format!(
            "    operation: {descriptor},\n    request: (params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Request> =>\n      this[resourceExecutor].request({descriptor}, params, options),\n    response: (params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Response> =>\n      this[resourceExecutor].response({descriptor}, params, options),\n  }});\n"
        ));
        methods.push(format!("{}.{name}", path.join(".")));
    }
    source.push_str("}\n");
    Ok(())
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
