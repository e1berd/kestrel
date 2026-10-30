use std::path::Path;

use crate::{DataLayer, Operation};

use super::aggregate::import_path;

pub(super) fn wrapper(adapter: DataLayer) -> Option<(&'static str, &'static str)> {
    match adapter {
        DataLayer::None | DataLayer::Rstore => None,
        DataLayer::TanstackQueryReact => Some(("withQueryOptions", "lib/tanstack.js")),
        DataLayer::TanstackQueryVue => Some(("withVueQueryOptions", "lib/tanstack-vue.js")),
    }
}

pub(super) struct Typings {
    adapter: DataLayer,
    core: String,
    vue: String,
}

impl Typings {
    pub(super) fn for_module(adapter: DataLayer, module: &Path) -> Self {
        let prefix = |library: &str| format!("import('{}').", import_path(module, Path::new(library)));
        Self { adapter, core: prefix("lib/tanstack.js"), vue: prefix("lib/tanstack-vue.js") }
    }

    pub(super) fn bundled(adapter: DataLayer) -> Self {
        Self { adapter, core: String::new(), vue: String::new() }
    }

    pub(super) fn members(&self, operation: &Operation, argument: &str, parameters: &str) -> String {
        if !self.adapter.uses_tanstack_query() {
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
            DataLayer::TanstackQueryVue => (format!("{vue}VueQueryOptions<{output}>"), format!("{vue}ReactiveParams<{parameters}>")),
            _ => (format!("{core}QueryOptions<{output}>"), parameters.to_string()),
        };
        format!(
            "  queryOptions<Result = unknown>({argument}: {accepted}, options?: RequestOptions): {options};\n  queryKey(params?: {accepted}, options?: RequestOptions): readonly unknown[];\n"
        )
    }
}
