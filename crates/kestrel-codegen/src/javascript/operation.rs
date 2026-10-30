use std::path::Path;

use anyhow::Result;

use crate::{DataLayer, Operation};

use super::query::{self, Typings};

use super::aggregate::import_path;
use super::{Files, insert};

pub(super) fn generate(files: &mut Files, operation: &Operation, path: &Path, data_layer: DataLayer) -> Result<()> {
    let web = import_path(path, Path::new("lib/web.js"));
    let collection = import_path(path, Path::new("lib/collection.js"));
    let callable_module = import_path(path, Path::new("lib/callable.js"));
    let mut javascript = format!("import {{ defineOperation }} from '{web}';\nimport {{ createCallable }} from '{callable_module}';\n");
    let wrapper = query::wrapper(data_layer);
    if let Some((symbol, library)) = wrapper {
        let adapter = import_path(path, Path::new(library));
        javascript.push_str(&format!("import {{ {symbol} }} from '{adapter}';\n"));
    }
    let settings = match &operation.collection {
        Some(settings) => serde_json::to_string(settings)?,
        None => "null".into(),
    };
    let creation = format!("createCallable(execute, operation, {settings})");
    let creation = match wrapper {
        Some((symbol, _)) => format!("{symbol}({creation}, execute)"),
        None => creation,
    };
    javascript.push_str(&format!("\nexport const operation = defineOperation({});\n", serde_json::to_string(&operation.transport)?));
    javascript.push_str(&format!(
        "\n/**\n * @param {{import('{collection}').Executor}} execute\n * @returns {{import('./{}').Callable}}\n */\nexport function createOperation(execute) {{\n  return {creation};\n}}\n",
        path.file_name().and_then(|name| name.to_str()).expect("an ASCII operation filename")
    ));
    let collection_type = if operation.collection.is_some() { ", Collection" } else { "" };
    let json_type = if operation.body.as_ref().is_some_and(|body| body.contains("JsonValue")) { ", JsonValue" } else { "" };
    let mut declarations = format!(
        "import type {{ QueryValue, SearchParams, RequestOptions, Operation{json_type} }} from '{web}';\nimport type {{ Executor{collection_type} }} from '{collection}';\nexport declare const operation: Operation;\n"
    );
    declarations.push_str(&callable(operation, "Callable", "typeof operation", &Typings::for_module(data_layer, path))?);
    declarations.push_str("export declare function createOperation(execute: Executor): Callable;\n");
    insert(files, path, javascript, declarations);
    Ok(())
}

pub(super) fn callable(operation: &Operation, name: &str, descriptor: &str, typings: &Typings) -> Result<String> {
    let optional = operation.path_parameters.is_empty() && operation.body.is_none();
    let parameters = format!("{name}Params");
    let argument = if optional { "params?" } else { "params" };
    let output = if operation.collection.is_some() { "Collection<Result>" } else { "Promise<Result>" };
    let mut declarations = format!("export type {parameters} = {};\n", super::parameters(operation)?);
    declarations.push_str(&format!(
        "export interface {name} {{\n  <Result = unknown>({argument}: {parameters}, options?: RequestOptions): {output};\n  readonly operation: {descriptor};\n  request({argument}: {parameters}, options?: RequestOptions): Promise<Request>;\n  response({argument}: {parameters}, options?: RequestOptions): Promise<Response>;\n"
    ));
    declarations.push_str(&typings.members(operation, argument, &parameters));
    declarations.push_str("}\n");
    Ok(declarations)
}
