use std::path::Path;

use crate::QueryAdapter;

use super::{Files, insert};

pub(super) fn files(query: QueryAdapter) -> Files {
    let mut files = Files::new();
    macro_rules! template {
        ($name:literal) => {
            insert(
                &mut files,
                Path::new(concat!("lib/", $name, ".js")),
                include_str!(concat!("templates/", $name, ".js")).into(),
                include_str!(concat!("templates/", $name, ".d.ts")).into(),
            );
        };
    }
    template!("web");
    template!("runtime");
    template!("routing");
    template!("auth");
    template!("cookies");
    template!("hooks");
    template!("events");
    template!("executor");
    template!("collection");
    template!("callable");
    if query.is_enabled() {
        template!("tanstack");
    }
    if query == QueryAdapter::Vue {
        template!("tanstack-vue");
    }
    files
}
