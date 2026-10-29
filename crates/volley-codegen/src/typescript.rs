use anyhow::Result;
use volley_core::transfer::Progress;

use crate::{Operation, Resource};

pub(crate) fn generate(root: &Resource, progress: &Progress) -> Result<(String, Vec<String>)> {
    let mut source = include_str!("runtime.ts").to_string();
    source.push_str(
        "\nexport function createClient(options: ClientOptions = {}) {\n  const execute = createExecutor(options);\n  return {\n",
    );
    let mut methods = Vec::new();
    render(root, &mut source, &mut methods, "", 4, progress)?;
    source.push_str("  };\n}\n\nexport type Client = ReturnType<typeof createClient>;\n");
    Ok((source, methods))
}

fn render(root: &Resource, source: &mut String, methods: &mut Vec<String>, prefix: &str, depth: usize, progress: &Progress) -> Result<()> {
    let indent = " ".repeat(depth);
    for (name, child) in &root.resources {
        progress.check()?;
        source.push_str(&format!("{indent}{name}: {{\n"));
        render(child, source, methods, &format!("{prefix}{name}."), depth + 2, progress)?;
        source.push_str(&format!("{indent}}},\n"));
    }
    for (name, operation) in &root.methods {
        progress.check()?;
        let parameters = parameters(operation)?;
        let default = if operation.path_parameters.is_empty() && operation.body.is_none() { " = {}" } else { "" };
        let descriptor = serde_json::to_string(&operation.transport)?;
        source.push_str(&format!(
            "{indent}{name}: <Response = unknown>(params: {parameters}{default}, options: RequestOptions = {{}}): Promise<Response> =>\n{indent}  execute<Response>({descriptor}, params, options),\n"
        ));
        methods.push(format!("{prefix}{name}"));
    }
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
    Ok(format!("{{ {} }}", fields.join("; ")))
}
