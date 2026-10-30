use std::path::{Path, PathBuf};

pub(super) const GROUP_SIZE: usize = 64;
use super::{Files, insert};

pub(super) fn import_path(from: &Path, to: &Path) -> String {
    let parent: Vec<_> = from.parent().expect("a generated module has a parent").iter().collect();
    let target = to.with_extension("js");
    let target: Vec<_> = target.iter().collect();
    let common = parent.iter().zip(&target).take_while(|(left, right)| left == right).count();
    let mut parts = vec!["..".to_string(); parent.len() - common];
    parts.extend(target[common..].iter().map(|part| part.to_string_lossy().into_owned()));
    let path = parts.join("/");
    if path.starts_with("../") { path } else { format!("./{path}") }
}

pub(super) fn records(files: &mut Files, root: &Path, mut parts: Vec<PathBuf>, factory: bool) {
    let directory = root.parent().expect("a registry directory");
    let mut level = 0;
    while parts.len() > GROUP_SIZE {
        parts = parts
            .chunks(GROUP_SIZE)
            .enumerate()
            .map(|(index, children)| {
                let path = directory.join(format!("group-{level}-{index}.js"));
                merge(files, &path, children, factory);
                path
            })
            .collect();
        level += 1;
    }
    merge(files, root, &parts, factory);
}

fn merge(files: &mut Files, path: &Path, parts: &[PathBuf], factory: bool) {
    let symbol = if factory { "createMembers" } else { "operations" };
    let mut source = String::new();
    for (index, part) in parts.iter().enumerate() {
        source.push_str(&format!("import {{ {symbol} as part{index} }} from '{}';\n", import_path(path, part)));
    }
    let mut declarations = source.clone();
    source.push_str(if factory {
        "\nexport function createMembers(execute) {\n  return Object.freeze({\n"
    } else {
        "\nexport const operations = Object.freeze({\n"
    });
    let mut types = Vec::new();
    for index in 0..parts.len() {
        source.push_str(&format!("  ...part{index}{},\n", if factory { "(execute)" } else { "" }));
        types.push(if factory { format!("ReturnType<typeof part{index}>") } else { format!("typeof part{index}") });
    }
    source.push_str(if factory { "  });\n}\n" } else { "});\n" });
    let combined = if types.is_empty() { "{}".into() } else { types.join(" & ") };
    if factory {
        declarations.push_str(&format!(
            "import type {{ Executor }} from '{}';\nexport declare function createMembers(execute: Executor): {combined};\n",
            import_path(path, Path::new("lib/collection.js"))
        ));
    } else {
        declarations.push_str(&format!("export declare const operations: Readonly<{combined}>;\n"));
    }
    insert(files, path, source, declarations);
}

pub(super) fn exports(files: &mut Files, directory: &Path, mut parts: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut level = 0;
    while parts.len() > GROUP_SIZE {
        parts = parts
            .chunks(GROUP_SIZE)
            .enumerate()
            .map(|(index, children)| {
                let path = directory.join(format!("exports-{level}-{index}.js"));
                let source: String = children.iter().map(|part| format!("export * from '{}';\n", import_path(&path, part))).collect();
                insert(files, &path, source.clone(), source);
                path
            })
            .collect();
        level += 1;
    }
    parts
}
