use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use kestrel_core::transfer::Progress;

use crate::{DataLayer, Resource};

use super::aggregate::{self, GROUP_SIZE, import_path};
use super::{Files, insert};

struct Member {
    name: String,
    module: PathBuf,
    symbol: String,
    constructor: bool,
}

#[derive(Default)]
struct Modules {
    files: Files,
    classes: Vec<PathBuf>,
    operations: Vec<(String, PathBuf)>,
    methods: Vec<String>,
}

pub(super) fn generate(
    root: &Resource,
    services: &BTreeMap<String, String>,
    plugins: &[(&str, &str, &str)],
    data_layer: DataLayer,
    progress: &Progress,
) -> Result<(Files, Vec<String>, Vec<String>)> {
    let mut modules = Modules { files: super::runtime::files(data_layer), ..Default::default() };
    let mut names = BTreeMap::new();
    super::register(root, &[], &mut names, &mut BTreeSet::new());
    resources(root, &[], &names, &mut modules, data_layer, progress)?;
    registry(&mut modules)?;
    let mut warnings = Vec::new();
    let mut exports =
        vec![PathBuf::from("client.js"), "services.js".into(), "operations.js".into(), "generation.js".into(), "lib/web.js".into()];
    if data_layer == DataLayer::Rstore {
        let rstore = super::rstore::generate(root)?;
        warnings.extend(rstore.skipped);
        let javascript =
            format!("import {{ withItemType }} from '@rstore/vue';\nimport {{ ApiError }} from './lib/web.js';\n{}", rstore.javascript);
        let declarations = format!("import type {{ Client }} from './client.js';\n{}", rstore.declarations);
        insert(&mut modules.files, Path::new("rstore.js"), javascript, declarations);
        exports.push("rstore.js".into());
    }
    let service_values = serde_json::to_string(services)?;
    let service_types = services
        .iter()
        .map(|(name, origin)| Ok(format!("readonly {}: {}", serde_json::to_string(name)?, serde_json::to_string(origin)?)))
        .collect::<Result<Vec<_>>>()?
        .join("; ");
    insert(
        &mut modules.files,
        Path::new("services.js"),
        format!("export const services = Object.freeze({service_values});\n"),
        format!("export declare const services: {{ {service_types} }};\nexport type ServiceName = keyof typeof services;\n"),
    );
    let plugin_names: Vec<_> = plugins.iter().map(|(name, _, _)| name).collect();
    let generation = format!(
        "export const generation = Object.freeze({{ version: 1, plugins: Object.freeze({}) }});\n",
        serde_json::to_string(&plugin_names)?
    );
    insert(
        &mut modules.files,
        Path::new("generation.js"),
        generation,
        "export declare const generation: { readonly version: 1; readonly plugins: readonly string[] };\n".into(),
    );
    let index = Path::new("index.js");
    exports.extend(modules.classes);
    let mut source = aggregate::exports(&mut modules.files, Path::new("exports"), exports)
        .iter()
        .map(|path| format!("export * from '{}';\n", import_path(index, path)))
        .collect::<String>();
    source.push_str("export { Resource, Collection } from './lib/collection.js';\n");
    let mut declarations = source.clone();
    declarations.push_str("export type { Page } from './lib/collection.js';\n");
    for (position, (_, runtime, types)) in plugins.iter().enumerate() {
        if runtime.is_empty() {
            continue;
        }
        let path = PathBuf::from(format!("plugins/plugin-{position}.js"));
        let mut plugin = String::new();
        for (symbol, module) in [("defineClientPlugin", "lib/web.js"), ("operations", "operations.js"), ("services", "services.js")] {
            if runtime.contains(symbol) {
                plugin.push_str(&format!("import {{ {symbol} }} from '{}';\n", import_path(&path, Path::new(module))));
            }
        }
        plugin.push_str(runtime);
        plugin.push('\n');
        insert(&mut modules.files, &path, plugin, if types.is_empty() { "export {};\n".into() } else { types.to_string() });
        declarations.push_str(&format!("export * from '{}';\n", import_path(index, &path)));
        source.push_str(&format!("export * from '{}';\n", import_path(index, &path)));
    }
    insert(&mut modules.files, index, source, declarations);
    Ok((modules.files, modules.methods, warnings))
}

fn resources(
    root: &Resource,
    path: &[String],
    names: &BTreeMap<Vec<String>, String>,
    modules: &mut Modules,
    data_layer: DataLayer,
    progress: &Progress,
) -> Result<PathBuf> {
    let class = &names[path];
    let target = if path.is_empty() { PathBuf::from("client.js") } else { PathBuf::from(format!("resources/{class}.js")) };
    let mut members = Vec::new();
    for (name, child) in &root.resources {
        let mut child_path = path.to_vec();
        child_path.push(name.clone());
        let module = resources(child, &child_path, names, modules, data_layer, progress)?;
        members.push(Member { name: name.clone(), module, symbol: names[&child_path].clone(), constructor: true });
    }
    let mut filenames = BTreeSet::new();
    for (name, operation) in &root.methods {
        progress.check()?;
        let mut filename = format!("operation-{name}");
        let mut suffix = 2;
        while !filenames.insert(filename.to_ascii_lowercase()) {
            filename = format!("operation-{name}-{suffix}");
            suffix += 1;
        }
        let module = PathBuf::from(format!("operations/{class}/{filename}.js"));
        super::operation::generate(&mut modules.files, operation, &module, data_layer)?;
        modules.operations.push((operation.transport.id.clone(), module.clone()));
        modules.methods.push(operation.transport.id.clone());
        members.push(Member { name: name.clone(), module, symbol: "createOperation".into(), constructor: false });
    }
    let directory = PathBuf::from(format!("members/{class}"));
    let member_root = directory.join("index.js");
    let mut parts = Vec::new();
    for (part, members) in members.chunks(GROUP_SIZE).enumerate() {
        progress.check()?;
        let file = directory.join(format!("part-{part}.js"));
        let mut source = String::new();
        for (position, member) in members.iter().enumerate() {
            source.push_str(&format!(
                "import {{ {} as member{position} }} from '{}';\n",
                member.symbol,
                import_path(&file, &member.module)
            ));
        }
        let mut declarations = source.clone();
        declarations.push_str(&format!(
            "import type {{ Executor }} from '{}';\nexport declare function createMembers(execute: Executor): Readonly<{{\n",
            import_path(&file, Path::new("lib/collection.js"))
        ));
        source.push_str("\nexport function createMembers(execute) {\n  return Object.freeze({\n");
        for (position, member) in members.iter().enumerate() {
            let member_type = if member.constructor {
                format!("InstanceType<typeof member{position}>")
            } else {
                format!("ReturnType<typeof member{position}>")
            };
            declarations.push_str(&format!("  readonly {}: {member_type};\n", serde_json::to_string(&member.name)?));
            source.push_str(&format!(
                "    {}: {}member{position}(execute),\n",
                serde_json::to_string(&member.name)?,
                if member.constructor { "new " } else { "" }
            ));
        }
        source.push_str("  });\n}\n");
        declarations.push_str("}>;\n");
        insert(&mut modules.files, &file, source, declarations);
        parts.push(file);
    }
    aggregate::records(&mut modules.files, &member_root, parts, true);
    let mut source = format!(
        "import {{ Resource }} from '{}';\nimport {{ createMembers }} from '{}';\n",
        import_path(&target, Path::new("lib/collection.js")),
        import_path(&target, &member_root)
    );
    let mut declarations = format!(
        "import {{ Resource, type Executor }} from '{}';\nimport {{ createMembers }} from '{}';\n",
        import_path(&target, Path::new("lib/collection.js")),
        import_path(&target, &member_root)
    );
    let argument = if path.is_empty() { "options?: ClientOptions" } else { "execute: Executor" };
    if path.is_empty() {
        source.push_str("import { createExecutor } from './lib/executor.js';\n");
        declarations.push_str("import type { ClientOptions } from './lib/web.js';\n");
    }
    declarations.push_str(&format!("export interface {class} extends ReturnType<typeof createMembers> {{}}\nexport declare class {class} extends Resource {{\n  constructor({argument});\n}}\n"));
    source.push_str(&format!("\nexport class {class} extends Resource {{\n"));
    if path.is_empty() {
        source.push_str("  /** @param {import('./lib/web.js').ClientOptions} [options] */\n  constructor(options = {}) {\n    const execute = createExecutor(options);\n");
    } else {
        source.push_str(&format!(
            "  /** @param {{import('{}').Executor}} execute */\n  constructor(execute) {{\n",
            import_path(&target, Path::new("lib/collection.js"))
        ));
    }
    source.push_str("    super(execute);\n    Object.assign(this, createMembers(execute));\n  }\n}\n");
    if path.is_empty() {
        source.push_str("\n/** @param {import('./lib/web.js').ClientOptions} [options] */\nexport function createClient(options = {}) {\n  return new Client(options);\n}\n");
        declarations.push_str("export declare function createClient(options?: ClientOptions): Client;\n");
    } else {
        modules.classes.push(target.clone());
    }
    insert(&mut modules.files, &target, source, declarations);
    Ok(target)
}

fn registry(modules: &mut Modules) -> Result<()> {
    let mut parts = Vec::new();
    for (part, operations) in modules.operations.chunks(GROUP_SIZE).enumerate() {
        let path = PathBuf::from(format!("registries/operations/part-{part}.js"));
        let mut source = String::new();
        for (position, (_, operation)) in operations.iter().enumerate() {
            source.push_str(&format!("import {{ operation as operation{position} }} from '{}';\n", import_path(&path, operation)));
        }
        source.push_str("\nexport const operations = Object.freeze({\n");
        for (position, (id, _)) in operations.iter().enumerate() {
            source.push_str(&format!("  {}: operation{position},\n", serde_json::to_string(id)?));
        }
        source.push_str("});\n");
        let mut declarations = source[..source.find("\nexport const").expect("registry exports")].to_string();
        declarations.push_str("\nexport declare const operations: Readonly<{\n");
        for (position, (id, _)) in operations.iter().enumerate() {
            declarations.push_str(&format!("  {}: typeof operation{position};\n", serde_json::to_string(id)?));
        }
        declarations.push_str("}>;\n");
        insert(&mut modules.files, &path, source, declarations);
        parts.push(path);
    }
    let aggregate = Path::new("registries/operations/index.js");
    aggregate::records(&mut modules.files, aggregate, parts, false);
    insert(&mut modules.files, Path::new("operations.js"), "export { operations } from './registries/operations/index.js';\n".into(), "export { operations } from './registries/operations/index.js';\nimport type { operations } from './registries/operations/index.js';\nexport type OperationId = keyof typeof operations;\n".into());
    Ok(())
}
