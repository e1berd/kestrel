use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use volley_core::transfer::Progress;

use crate::Resource;

use super::aggregate::{self, Files, GROUP_SIZE, import_path};

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
    plugins: &[(&str, &str)],
    progress: &Progress,
) -> Result<(Files, Vec<String>)> {
    let mut modules = Modules { files: super::runtime::files(), ..Default::default() };
    let mut names = BTreeMap::new();
    super::register(root, &[], &mut names, &mut BTreeSet::new());
    resources(root, &[], &names, &mut modules, progress)?;
    registry(&mut modules)?;
    modules.files.insert(
        "services.ts".into(),
        format!(
            "export const services = Object.freeze({} as const);\nexport type ServiceName = keyof typeof services;\n",
            serde_json::to_string(services)?
        ),
    );
    let plugin_names: Vec<_> = plugins.iter().map(|(name, _)| name).collect();
    modules.files.insert(
        "generation.ts".into(),
        format!(
            "export const generation = Object.freeze({{ version: 1, plugins: Object.freeze({}) }});\n",
            serde_json::to_string(&plugin_names)?
        ),
    );
    let index = Path::new("index.ts");
    let mut exports =
        vec![PathBuf::from("client.ts"), "services.ts".into(), "operations.ts".into(), "generation.ts".into(), "lib/web.ts".into()];
    exports.extend(modules.classes);
    let mut source = aggregate::exports(&mut modules.files, Path::new("exports"), exports)
        .iter()
        .map(|path| format!("export * from '{}';\n", import_path(index, path)))
        .collect::<String>();
    source.push_str("export { Resource, Collection } from './lib/collection.js';\nexport type { Page } from './lib/collection.js';\n");
    for (position, (_, runtime)) in plugins.iter().enumerate() {
        if runtime.is_empty() {
            continue;
        }
        let path = PathBuf::from(format!("plugins/plugin-{position}.ts"));
        let mut plugin = String::new();
        for (symbol, module) in [("defineClientPlugin", "lib/web.ts"), ("operations", "operations.ts"), ("services", "services.ts")] {
            if runtime.contains(symbol) {
                plugin.push_str(&format!("import {{ {symbol} }} from '{}';\n", import_path(&path, Path::new(module))));
            }
        }
        plugin.push_str(runtime);
        plugin.push('\n');
        modules.files.insert(path.clone(), plugin);
        source.push_str(&format!("export * from '{}';\n", import_path(index, &path)));
    }
    modules.files.insert(index.to_path_buf(), source);
    Ok((modules.files, modules.methods))
}

fn resources(
    root: &Resource,
    path: &[String],
    names: &BTreeMap<Vec<String>, String>,
    modules: &mut Modules,
    progress: &Progress,
) -> Result<PathBuf> {
    let class = &names[path];
    let target = if path.is_empty() { PathBuf::from("client.ts") } else { PathBuf::from(format!("resources/{class}.ts")) };
    let mut members = Vec::new();
    for (name, child) in &root.resources {
        let mut child_path = path.to_vec();
        child_path.push(name.clone());
        let module = resources(child, &child_path, names, modules, progress)?;
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
        let module = PathBuf::from(format!("operations/{class}/{filename}.ts"));
        modules.files.insert(module.clone(), super::operation::source(operation, &module)?);
        modules.operations.push((operation.transport.id.clone(), module.clone()));
        modules.methods.push(operation.transport.id.clone());
        members.push(Member { name: name.clone(), module, symbol: "createOperation".into(), constructor: false });
    }
    let directory = PathBuf::from(format!("members/{class}"));
    let member_root = directory.join("index.ts");
    let mut parts = Vec::new();
    for (part, members) in members.chunks(GROUP_SIZE).enumerate() {
        progress.check()?;
        let file = directory.join(format!("part-{part}.ts"));
        let mut source = format!("import type {{ Executor }} from '{}';\n", import_path(&file, Path::new("lib/collection.ts")));
        for (position, member) in members.iter().enumerate() {
            source.push_str(&format!(
                "import {{ {} as member{position} }} from '{}';\n",
                member.symbol,
                import_path(&file, &member.module)
            ));
        }
        source.push_str("\nexport function createMembers(execute: Executor) {\n  return Object.freeze({\n");
        for (position, member) in members.iter().enumerate() {
            source.push_str(&format!(
                "    {}: {}member{position}(execute),\n",
                serde_json::to_string(&member.name)?,
                if member.constructor { "new " } else { "" }
            ));
        }
        source.push_str("  });\n}\n");
        modules.files.insert(file.clone(), source);
        parts.push(file);
    }
    aggregate::records(&mut modules.files, &member_root, parts, true);
    let executor = if path.is_empty() { "" } else { ", type Executor" };
    let mut source = format!(
        "import {{ Resource{executor} }} from '{}';\nimport {{ createMembers }} from '{}';\n",
        import_path(&target, Path::new("lib/collection.ts")),
        import_path(&target, &member_root)
    );
    if path.is_empty() {
        source.push_str("import { createExecutor } from './lib/executor.js';\nimport type { ClientOptions } from './lib/web.js';\n");
    }
    source.push_str(&format!(
        "\nexport interface {class} extends ReturnType<typeof createMembers> {{}}\nexport class {class} extends Resource {{\n"
    ));
    source.push_str(if path.is_empty() {
        "  constructor(options: ClientOptions = {}) {\n    const execute = createExecutor(options);\n"
    } else {
        "  constructor(execute: Executor) {\n"
    });
    source.push_str("    super(execute);\n    Object.assign(this, createMembers(execute));\n  }\n}\n");
    if path.is_empty() {
        source.push_str("\nexport function createClient(options: ClientOptions = {}): Client {\n  return new Client(options);\n}\n");
    } else {
        modules.classes.push(target.clone());
    }
    modules.files.insert(target.clone(), source);
    Ok(target)
}

fn registry(modules: &mut Modules) -> Result<()> {
    let mut parts = Vec::new();
    for (part, operations) in modules.operations.chunks(GROUP_SIZE).enumerate() {
        let path = PathBuf::from(format!("registries/operations/part-{part}.ts"));
        let mut source = String::new();
        for (position, (_, operation)) in operations.iter().enumerate() {
            source.push_str(&format!("import {{ operation as operation{position} }} from '{}';\n", import_path(&path, operation)));
        }
        source.push_str("\nexport const operations = Object.freeze({\n");
        for (position, (id, _)) in operations.iter().enumerate() {
            source.push_str(&format!("  {}: operation{position},\n", serde_json::to_string(id)?));
        }
        source.push_str("});\n");
        modules.files.insert(path.clone(), source);
        parts.push(path);
    }
    let aggregate = Path::new("registries/operations/index.ts");
    aggregate::records(&mut modules.files, aggregate, parts, false);
    modules.files.insert("operations.ts".into(), "export { operations } from './registries/operations/index.js';\nimport type { operations } from './registries/operations/index.js';\nexport type OperationId = keyof typeof operations;\n".into());
    Ok(())
}
