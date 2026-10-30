use std::path::Path;

use crate::{Operation, QueryAdapter};

use super::aggregate::import_path;

pub(super) fn wrapper(adapter: QueryAdapter) -> Option<(&'static str, &'static str)> {
    match adapter {
        QueryAdapter::None => None,
        QueryAdapter::React => Some(("withQueryOptions", "lib/tanstack.js")),
        QueryAdapter::Vue => Some(("withVueQueryOptions", "lib/tanstack-vue.js")),
    }
}

pub(super) struct Typings {
    adapter: QueryAdapter,
    core: String,
    vue: String,
}

impl Typings {
    pub(super) fn for_module(adapter: QueryAdapter, module: &Path) -> Self {
        let prefix = |library: &str| format!("import('{}').", import_path(module, Path::new(library)));
        Self { adapter, core: prefix("lib/tanstack.js"), vue: prefix("lib/tanstack-vue.js") }
    }

    pub(super) fn bundled(adapter: QueryAdapter) -> Self {
        Self { adapter, core: String::new(), vue: String::new() }
    }

    pub(super) fn members(&self, operation: &Operation, argument: &str, parameters: &str) -> String {
        if !self.adapter.is_enabled() {
            return String::new();
        }
        let output = if operation.collection.is_some() { "Result[]" } else { "Result" };
        let (core, vue) = (&self.core, &self.vue);
        if !matches!(operation.transport.method.as_str(), "GET" | "HEAD" | "QUERY") {
            return format!(
                "  mutationOptions<Result = unknown>(options?: RequestOptions): {core}MutationOptions<{parameters}, {output}>;\n"
            );
        }
        let (options, accepted) = match self.adapter {
            QueryAdapter::Vue => (format!("{vue}VueQueryOptions<{output}>"), format!("{vue}ReactiveParams<{parameters}>")),
            _ => (format!("{core}QueryOptions<{output}>"), parameters.to_string()),
        };
        format!(
            "  queryOptions<Result = unknown>({argument}: {accepted}, options?: RequestOptions): {options};\n  queryKey(params?: {accepted}, options?: RequestOptions): readonly unknown[];\n"
        )
    }
}
