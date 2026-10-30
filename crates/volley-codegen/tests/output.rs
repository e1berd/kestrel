use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use volley_codegen::{OutputOptions, generate, generate_to};
use volley_core::format::RequestFile;
use volley_core::transfer::Progress;
use volley_core::{Collection, Document, Item, Request};

struct Fixture {
    directory: PathBuf,
    collection: Collection,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("kestrel-output-{}", uuid::Uuid::new_v4()));
        let mut collection = Collection::create(&directory.join("collection"), "Output").unwrap();
        let root = collection.root.clone();
        collection.add_request(&root, RequestFile::new_http("Get user", "GET", "https://example.com/users/:user_id")).unwrap();
        Self { directory, collection }
    }

    fn write(&self, destination: &Path, options: OutputOptions, overwrite: bool) -> anyhow::Result<volley_codegen::Generated> {
        generate_to(&self.collection.root, destination, options, overwrite, &Progress::default())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).expect("remove output fixtures");
    }
}

fn read_zip(path: &Path) -> BTreeMap<PathBuf, String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let path = entry.enclosed_name().expect("a safe relative ZIP entry");
        let mut source = String::new();
        entry.read_to_string(&mut source).unwrap();
        files.insert(path, source);
    }
    files
}

#[test]
fn the_default_is_a_directory_and_the_two_output_flags_are_independent() {
    let fixture = Fixture::new();
    assert_eq!(OutputOptions::default(), OutputOptions { single_file: false, archive: false });
    for single_file in [false, true] {
        for archive in [false, true] {
            let options = OutputOptions { single_file, archive };
            let name = if archive {
                format!("sdk-{single_file}.zip")
            } else if single_file {
                "client.ts".into()
            } else {
                "sdk".into()
            };
            let destination = fixture.directory.join(name);
            let generated = fixture.write(&destination, options, false).unwrap();
            assert_eq!(generated.entrypoint, Path::new(if single_file { "client.ts" } else { "index.ts" }));
            assert_eq!(generated.files.len() == 1, single_file);
            if archive {
                let mut files = read_zip(&destination);
                if !single_file {
                    let marker = files.remove(Path::new(".kestrel-codegen.json")).unwrap();
                    assert_eq!(serde_json::from_str::<serde_json::Value>(&marker).unwrap()["version"], 1);
                }
                assert_eq!(files, generated.files);
                let original = std::fs::read(&destination).unwrap();
                fixture.write(&destination, options, true).unwrap();
                assert_eq!(std::fs::read(&destination).unwrap(), original);
            } else if single_file {
                assert_eq!(std::fs::read_to_string(&destination).unwrap(), generated.files[&generated.entrypoint]);
            } else {
                for (path, source) in &generated.files {
                    assert_eq!(std::fs::read_to_string(destination.join(path)).unwrap(), *source);
                }
                assert!(destination.join("lib/web.ts").is_file());
                assert!(destination.join("resources/UsersResource.ts").is_file());
            }
            assert!(fixture.write(&destination, options, false).is_err());
        }
    }
}

#[test]
fn directory_replacement_removes_stale_generated_files_and_preserves_unrelated_data() {
    let mut fixture = Fixture::new();
    let root = fixture.collection.root.clone();
    let obsolete =
        fixture.collection.add_request(&root, RequestFile::new_http("List orders", "GET", "https://example.com/orders")).unwrap();
    let destination = fixture.directory.join("sdk");
    std::fs::create_dir(&destination).unwrap();
    let first = fixture.write(&destination, OutputOptions::default(), false).unwrap();
    fixture.collection.remove(&obsolete).unwrap();
    let second = fixture.write(&destination, OutputOptions::default(), true).unwrap();
    for path in first.files.keys().filter(|path| !second.files.contains_key(*path)) {
        assert!(!destination.join(path).exists(), "stale generated module: {}", path.display());
    }
    std::fs::write(destination.join("notes.txt"), "keep my notes").unwrap();
    assert!(fixture.write(&destination, OutputOptions::default(), true).is_err());
    assert_eq!(std::fs::read_to_string(destination.join("notes.txt")).unwrap(), "keep my notes");
    for (path, source) in &second.files {
        assert_eq!(std::fs::read_to_string(destination.join(path)).unwrap(), *source);
    }
    let cancelled = Progress::default();
    cancelled.cancel();
    assert!(generate_to(&fixture.collection.root, &destination, OutputOptions::default(), true, &cancelled).is_err());
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 2);
    assert!(fixture.write(&fixture.collection.root, OutputOptions::default(), true).is_err());
}

#[test]
fn large_resource_groups_are_split_without_creating_giant_registries_or_resource_classes() {
    let mut fixture = Fixture::new();
    for index in 0..300 {
        let file = RequestFile::new_http(&format!("Action {index}"), "POST", &format!("https://example.com/users/:user_id:action{index}"));
        fixture
            .collection
            .items
            .push(Item::Request(Request { doc: Document::new(fixture.collection.root.join(format!("action-{index}.yml")), file, true) }));
    }
    for index in 0..80 {
        let file = RequestFile::new_http(&format!("Resource {index}"), "GET", &format!("https://example.com/resource{index}"));
        fixture
            .collection
            .items
            .push(Item::Request(Request { doc: Document::new(fixture.collection.root.join(format!("resource-{index}.yml")), file, true) }));
    }
    let generated = generate(&fixture.collection, &Progress::default()).unwrap();
    assert_eq!(generated.requests, 381);
    for (path, source) in &generated.files {
        assert!(source.lines().count() < 350, "oversized module: {}", path.display());
    }
    assert_eq!(generated.files, generate(&fixture.collection, &Progress::default()).unwrap().files);
}

#[cfg(unix)]
#[test]
fn directory_outputs_refuse_symlinks_instead_of_following_them() {
    let fixture = Fixture::new();
    let destination = fixture.directory.join("sdk");
    let generated = fixture.write(&destination, OutputOptions::default(), false).unwrap();
    std::os::unix::fs::symlink(&fixture.collection.root, destination.join("foreign")).unwrap();
    assert!(fixture.write(&destination, OutputOptions::default(), true).is_err());
    assert!(fixture.collection.root.join("opencollection.yml").is_file());
    let path = generated.files.keys().next().unwrap();
    assert!(destination.join(path).is_file());
}
