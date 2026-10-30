use std::path::Path;

use anyhow::Result;

use crate::Operation;

use super::aggregate::import_path;

pub(super) fn source(operation: &Operation, path: &Path) -> Result<String> {
    let json = if operation.body.as_ref().is_some_and(|body| body.contains("JsonValue")) { ", type JsonValue" } else { "" };
    let collection = if operation.collection.is_some() { "createCollection, type Collection, " } else { "" };
    let imports = format!(
        "import {{ defineOperation, type QueryValue, type SearchParams, type RequestOptions{json} }} from '{}';\nimport {{ {collection}type Executor }} from '{}';\n",
        import_path(path, Path::new("lib/web.ts")),
        import_path(path, Path::new("lib/collection.ts")),
    );
    let mut source = imports;
    source.push_str(&format!("\nexport const operation = defineOperation({});\n", serde_json::to_string(&operation.transport)?));
    source.push_str("\nexport function createOperation(execute: Executor) {\n");
    let parameters = super::parameters(operation)?;
    let default = if operation.path_parameters.is_empty() && operation.body.is_none() { " = {}" } else { "" };
    if let Some(collection) = &operation.collection {
        source.push_str(&format!(
            "  return Object.assign(<Item = unknown>(params: {parameters}{default}, options: RequestOptions = {{}}): Collection<Item> =>\n    createCollection<Item>(execute, operation, params, options, {}), {{\n",
            serde_json::to_string(collection)?,
        ));
    } else {
        source.push_str(&format!(
            "  return Object.assign(<Response = unknown>(params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Response> =>\n    execute<Response>(operation, params, options), {{\n"
        ));
    }
    source.push_str(&format!(
        "    operation,\n    request: (params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Request> =>\n      execute.request(operation, params, options),\n    response: (params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Response> =>\n      execute.response(operation, params, options),\n  }});\n}}\n"
    ));
    Ok(source)
}
