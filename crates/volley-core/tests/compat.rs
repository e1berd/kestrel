use std::path::PathBuf;

use volley_core::{Collection, Item};

fn collection_dir() -> PathBuf {
    std::env::var_os("KESTREL_COMPAT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/httpbin"))
}

#[test]
fn every_file_writes_back_as_it_was() {
    let collection = Collection::open(&collection_dir()).expect("open the collection");
    let mut checked = 0;
    let mut differ = Vec::new();
    let mut check = |path: &std::path::Path, rendered: String| {
        let on_disk = std::fs::read_to_string(path).unwrap();
        checked += 1;
        if on_disk != rendered {
            let line = on_disk
                .lines()
                .zip(rendered.lines())
                .position(|(a, b)| a != b)
                .unwrap_or(on_disk.lines().count().min(rendered.lines().count()));
            differ.push(format!(
                "{} at line {}:\n  disk: {:?}\n  ours: {:?}",
                path.display(),
                line + 1,
                on_disk.lines().nth(line).unwrap_or("<end>"),
                rendered.lines().nth(line).unwrap_or("<end>")
            ));
        }
    };
    check(&collection.doc.path, collection.doc.render().unwrap());
    for env in &collection.environments {
        check(&env.doc.path, env.doc.render().unwrap());
    }
    fn walk(items: &[Item], check: &mut dyn FnMut(&std::path::Path, String)) {
        for item in items {
            match item {
                Item::Folder(folder) => {
                    if folder.doc.path.exists() {
                        check(&folder.doc.path, folder.doc.render().unwrap());
                    }
                    walk(&folder.children, check);
                }
                Item::Request(request) => check(&request.doc.path, request.doc.render().unwrap()),
            }
        }
    }
    walk(&collection.items, &mut check);
    assert!(differ.is_empty(), "{} of {checked} files would change:\n{}", differ.len(), differ.join("\n"));
    assert!(checked > 0);
    for request in collection.requests() {
        assert!(!request.doc.is_dirty(), "{} reads as changed", request.doc.path.display());
    }
}
