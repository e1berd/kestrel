use std::path::Path;

use crate::DataLayer;

use super::{Files, insert};

pub(super) fn files(data_layer: DataLayer) -> Files {
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
    if data_layer.uses_tanstack_query() {
        template!("tanstack");
    }
    if data_layer == DataLayer::TanstackQueryVue {
        template!("tanstack-vue");
    }
    files
}
