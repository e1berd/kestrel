use std::path::Path;

use super::{Files, insert};

pub(super) fn files(tanstack_query: bool) -> Files {
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
    template!("executor");
    template!("collection");
    if tanstack_query {
        template!("tanstack");
    }
    files
}
