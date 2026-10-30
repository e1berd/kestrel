use std::path::PathBuf;

use anyhow::{Result, bail};
use volley_codegen::{CodegenPlugin, Generator, Resource};
use volley_core::Collection;
use volley_core::format::RequestFile;
use volley_core::transfer::Progress;

fn source(generated: &volley_codegen::Generated) -> String {
    generated.files.values().map(String::as_str).collect()
}

struct Directory(PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove generator plugin fixture");
    }
}

fn collection() -> (Directory, Collection) {
    let directory = Directory(std::env::temp_dir().join(format!("kestrel-plugin-{}", uuid::Uuid::new_v4())));
    let mut collection = Collection::create(&directory.0, "Plugin").unwrap();
    let root = collection.root.clone();
    collection.add_request(&root, RequestFile::new_http("List users", "GET", "{{baseUrl}}/users")).unwrap();
    (directory, collection)
}

struct Accounts;

impl CodegenPlugin for Accounts {
    fn name(&self) -> &str {
        "accounts"
    }

    fn transform(&self, resources: &mut Resource) -> Result<()> {
        let operation = resources.resources.get_mut("users").unwrap().methods.get_mut("list").unwrap();
        operation.transport.service = "accounts".into();
        operation.transport.origin = "https://accounts.example.com".into();
        operation.transport.headers.insert("X-Generated".into(), "accounts".into());
        Ok(())
    }

    fn runtime(&self) -> &str {
        "export const accountsAdapter = defineClientPlugin({ name: 'accounts' });\n"
    }
}

#[test]
fn plugins_transform_the_shared_operation_graph_without_changing_collection_files() {
    let (_directory, collection) = collection();
    let path = &collection.requests()[0].doc.path;
    let before = std::fs::read(path).unwrap();
    let generator = Generator::new().plugin(Accounts);
    let generated = generator.generate(&collection, &Progress::default()).unwrap();
    assert!(source(&generated).contains("\"accounts\":\"https://accounts.example.com\""));
    assert!(source(&generated).contains("accountsAdapter = defineClientPlugin"));
    assert!(source(&generated).contains("plugins: Object.freeze([\"accounts\"])"));
    assert_eq!(generated.methods, ["users.list"]);
    assert_eq!(generated.files, generator.generate(&collection, &Progress::default()).unwrap().files);
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert!(Generator::new().plugin(Accounts).plugin(Accounts).generate(&collection, &Progress::default()).is_err());
}

struct Failing;

impl CodegenPlugin for Failing {
    fn name(&self) -> &str {
        "failing"
    }

    fn transform(&self, _resources: &mut Resource) -> Result<()> {
        bail!("cannot transform this operation");
    }
}

#[test]
fn plugin_errors_keep_the_plugin_name_and_original_cause() {
    let (_directory, collection) = collection();
    let error = Generator::new().plugin(Failing).generate(&collection, &Progress::default()).unwrap_err();
    assert!(format!("{error:#}").contains("failing"));
    assert!(format!("{error:#}").contains("cannot transform"));
}

#[test]
fn services_with_the_same_name_cannot_silently_merge_different_origins() {
    let (_directory, mut collection) = collection();
    let root = collection.root.clone();
    let mut request = RequestFile::new_http("List invoices", "GET", "https://billing.example.com/invoices");
    request.extra.insert("extensions".into(), serde_yaml_ng::from_str("kestrel:\n  codegen:\n    service: default\n").unwrap());
    collection.add_request(&root, request).unwrap();
    let error = volley_codegen::generate(&collection, &Progress::default()).unwrap_err();
    assert!(format!("{error:#}").contains("refers to several origins"));
}
