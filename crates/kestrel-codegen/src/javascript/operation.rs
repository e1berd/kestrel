use std::path::Path;

use anyhow::Result;

use crate::Operation;

use super::aggregate::import_path;
use super::{Files, insert};

pub(super) fn generate(files: &mut Files, operation: &Operation, path: &Path, tanstack_query: bool) -> Result<()> {
    let web = import_path(path, Path::new("lib/web.js"));
    let collection = import_path(path, Path::new("lib/collection.js"));
    let mut javascript = format!("import {{ defineOperation }} from '{web}';\n");
    if operation.collection.is_some() {
        javascript.push_str(&format!("import {{ createCollection }} from '{collection}';\n"));
    }
    if tanstack_query {
        let adapter = import_path(path, Path::new("lib/tanstack.js"));
        javascript.push_str(&format!("import {{ withQueryOptions }} from '{adapter}';\n"));
    }
    javascript.push_str(&format!("\nexport const operation = defineOperation({});\n", serde_json::to_string(&operation.transport)?));
    javascript.push_str("\n/**\n * @param {import('");
    javascript.push_str(&collection);
    javascript.push_str("').Executor} execute\n * @returns {import('./");
    javascript.push_str(path.file_name().expect("an operation filename").to_str().expect("an ASCII filename"));
    javascript.push_str("').Callable}\n */\nexport function createOperation(execute) {\n");
    let optional = operation.path_parameters.is_empty() && operation.body.is_none();
    let default = if optional { " = {}" } else { "" };
    let invocation = if let Some(settings) = &operation.collection {
        format!("createCollection(execute, operation, params, options, {})", serde_json::to_string(settings)?)
    } else {
        "execute(operation, params, options)".into()
    };
    javascript.push_str(&format!(
        "  const call = Object.assign((params{default}, options = {{}}) =>\n    {invocation}, {{\n    operation,\n    request: (params{default}, options = {{}}) => execute.request(operation, params, options),\n    response: (params{default}, options = {{}}) => execute.response(operation, params, options),\n  }});\n"
    ));
    javascript.push_str(if tanstack_query { "  return withQueryOptions(call, execute);\n}\n" } else { "  return call;\n}\n" });
    let collection_type = if operation.collection.is_some() { ", Collection" } else { "" };
    let json_type = if operation.body.as_ref().is_some_and(|body| body.contains("JsonValue")) { ", JsonValue" } else { "" };
    let mut declarations = format!(
        "import type {{ QueryValue, SearchParams, RequestOptions, Operation{json_type} }} from '{web}';\nimport type {{ Executor{collection_type} }} from '{collection}';\nexport declare const operation: Operation;\n"
    );
    let adapter = format!("import('{}').", import_path(path, Path::new("lib/tanstack.js")));
    declarations.push_str(&callable(operation, "Callable", "typeof operation", tanstack_query.then_some(adapter.as_str()))?);
    declarations.push_str("export declare function createOperation(execute: Executor): Callable;\n");
    insert(files, path, javascript, declarations);
    Ok(())
}

pub(super) fn callable(operation: &Operation, name: &str, descriptor: &str, tanstack: Option<&str>) -> Result<String> {
    let optional = operation.path_parameters.is_empty() && operation.body.is_none();
    let parameters = super::parameters(operation)?;
    let argument = if optional { "params?" } else { "params" };
    let output = if operation.collection.is_some() { "Collection<Result>" } else { "Promise<Result>" };
    let mut declarations = format!(
        "export interface {name} {{\n  <Result = unknown>({argument}: {parameters}, options?: RequestOptions): {output};\n  readonly operation: {descriptor};\n  request({argument}: {parameters}, options?: RequestOptions): Promise<Request>;\n  response({argument}: {parameters}, options?: RequestOptions): Promise<Response>;\n"
    );
    if let Some(adapter) = tanstack {
        let query = matches!(operation.transport.method.as_str(), "GET" | "HEAD" | "QUERY");
        if query {
            let output = if operation.collection.is_some() { "Result[]" } else { "Result" };
            declarations.push_str(&format!(
                "  queryOptions<Result = unknown>({argument}: {parameters}, options?: RequestOptions): {adapter}QueryOptions<{output}>;\n"
            ));
        } else {
            let output = if operation.collection.is_some() { "Result[]" } else { "Result" };
            declarations.push_str(&format!(
                "  mutationOptions<Result = unknown>(options?: RequestOptions): {adapter}MutationOptions<{parameters}, {output}>;\n"
            ));
        }
    }
    declarations.push_str("}\n");
    Ok(declarations)
}
