use std::fs;
use std::path::{Path, PathBuf};

use kestrel_core::collection_move::MoveTarget;
use kestrel_core::format::RequestFile;
use kestrel_core::{Collection, Item};

struct Fixture {
    collection: Collection,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("kestrel-move-{}", uuid::Uuid::new_v4()));
        Self { collection: Collection::create(&root, "Moves").unwrap() }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.collection.root).expect("remove the test collection");
    }
}

fn request(collection: &mut Collection, directory: &Path, name: &str) -> PathBuf {
    collection.add_request(directory, RequestFile::new_http(name, "GET", "https://example.com")).unwrap()
}

fn snapshot(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(snapshot(&path));
        } else {
            let bytes = fs::read(&path).unwrap();
            files.push((path, bytes));
        }
    }
    files.sort();
    files
}

#[test]
fn requests_move_between_folders_and_keep_yaml_extensions_and_unknown_fields() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let folder = collection.add_folder(&root, "Target").unwrap();
    let first = request(collection, &folder, "First");
    let source = root.join("Original.yaml");
    let original = "info:\n  name: Original\n  type: http\n  seq: 1\n  custom: untouched\n\nhttp:\n  method: GET\n  url: https://example.com\n\nx-extension:\n  keep: true\n";
    fs::write(&source, original).unwrap();
    collection.reload().unwrap();

    let moved = collection.move_item(&source, &MoveTarget::Inside(folder.clone())).unwrap();

    assert_eq!(moved, folder.join("Original.yaml"));
    assert!(!source.exists());
    assert_eq!(fs::read_to_string(&moved).unwrap(), original.replace("seq: 1", "seq: 2"));
    let reopened = Collection::open(&root).unwrap();
    assert_eq!(reopened.requests().iter().map(|request| request.doc.path.clone()).collect::<Vec<_>>(), vec![first, moved]);
}

#[test]
fn folders_move_with_their_entire_subtree_and_can_return_to_the_root() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let source = collection.add_folder(&root, "Source").unwrap();
    let child = collection.add_folder(&source, "Child").unwrap();
    let original = request(collection, &child, "Nested");
    fs::write(child.join("notes.txt"), "kept alongside the request").unwrap();
    let original_request = fs::read(&original).unwrap();
    let target = collection.add_folder(&root, "Target").unwrap();

    let moved = collection.move_item(&source, &MoveTarget::Inside(target.clone())).unwrap();
    let nested = moved.join("Child/Nested.yml");

    assert!(!source.exists());
    assert_eq!(fs::read(&nested).unwrap(), original_request);
    assert_eq!(fs::read_to_string(moved.join("Child/notes.txt")).unwrap(), "kept alongside the request");
    assert_eq!(
        collection.ancestors(&nested).iter().map(|folder| folder.dir.clone()).collect::<Vec<_>>(),
        vec![target, moved.clone(), moved.join("Child")]
    );

    let restored = collection.move_item(&moved, &MoveTarget::Inside(root.clone())).unwrap();
    assert_eq!(restored, source);
    assert!(collection.request(&original).is_some());
    assert_eq!(fs::read(original).unwrap(), original_request);
}

#[test]
fn folders_move_into_nested_nonempty_folders_and_persist_the_hierarchy() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let source = collection.add_folder(&root, "Source").unwrap();
    let child = collection.add_folder(&source, "Child").unwrap();
    let original = request(collection, &child, "Nested");
    let original_bytes = fs::read(&original).unwrap();
    let parent = collection.add_folder(&root, "Parent").unwrap();
    let target = collection.add_folder(&parent, "Target").unwrap();
    let existing = collection.add_folder(&target, "Existing").unwrap();
    let existing_request = request(collection, &target, "Keep");
    let existing_bytes = fs::read(&existing_request).unwrap();

    assert!(collection.can_move_item(&source, &MoveTarget::Inside(target.clone())));
    let moved = collection.move_item(&source, &MoveTarget::Inside(target.clone())).unwrap();
    assert_eq!(moved, target.join("Source"));

    let reopened = Collection::open(&root).unwrap();
    let Some(Item::Folder(folder)) = reopened.item(&target) else { panic!("the target remains a folder") };
    assert_eq!(
        folder.children.iter().map(Item::path).collect::<Vec<_>>(),
        vec![existing.as_path(), moved.as_path(), existing_request.as_path()]
    );
    let nested = moved.join("Child/Nested.yml");
    assert_eq!(
        reopened.ancestors(&nested).iter().map(|folder| folder.dir.as_path()).collect::<Vec<_>>(),
        vec![parent.as_path(), target.as_path(), moved.as_path(), moved.join("Child").as_path()]
    );
    assert_eq!(fs::read(nested).unwrap(), original_bytes);
    assert_eq!(fs::read(existing_request).unwrap(), existing_bytes);
    assert!(!source.exists());
}

#[test]
fn requests_can_be_inserted_before_and_after_other_requests() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let first = request(collection, &root, "First");
    let second = request(collection, &root, "Second");
    let third = request(collection, &root, "Third");

    collection.move_item(&third, &MoveTarget::Before(first.clone())).unwrap();
    assert_eq!(collection.items.iter().map(Item::path).collect::<Vec<_>>(), vec![third.as_path(), first.as_path(), second.as_path()]);
    collection.move_item(&third, &MoveTarget::After(second.clone())).unwrap();
    assert_eq!(collection.items.iter().map(Item::path).collect::<Vec<_>>(), vec![first.as_path(), second.as_path(), third.as_path()]);

    let folder = collection.add_folder(&root, "Folder").unwrap();
    let nested = request(collection, &folder, "Nested");
    let moved = collection.move_item(&nested, &MoveTarget::Before(second.clone())).unwrap();
    assert_eq!(moved, root.join("Nested.yml"));
    assert_eq!(
        collection.requests().iter().map(|request| request.doc.path.as_path()).collect::<Vec<_>>(),
        vec![first.as_path(), moved.as_path(), second.as_path(), third.as_path()]
    );
}

#[test]
fn folders_can_be_reordered_without_changing_their_contents() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let first = collection.add_folder(&root, "First").unwrap();
    let second = collection.add_folder(&root, "Second").unwrap();
    let nested = request(collection, &second, "Nested");
    let original_request = fs::read(&nested).unwrap();

    collection.move_item(&second, &MoveTarget::Before(first.clone())).unwrap();
    let reopened = Collection::open(&root).unwrap();
    assert_eq!(reopened.items.iter().map(Item::path).collect::<Vec<_>>(), vec![second.as_path(), first.as_path()]);
    assert_eq!(fs::read(nested).unwrap(), original_request);
}

#[test]
fn name_collisions_never_overwrite_requests_or_merge_folders() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let target = collection.add_folder(&root, "Target").unwrap();
    let existing = request(collection, &target, "Same");
    let original_bytes = fs::read(&existing).unwrap();
    let source = request(collection, &root, "Same");
    let moved = collection.move_item(&source, &MoveTarget::Inside(target.clone())).unwrap();
    assert_eq!(moved, target.join("Same 1.yml"));
    assert_eq!(fs::read(existing).unwrap(), original_bytes);

    let folder = collection.add_folder(&root, "Version.1").unwrap();
    let existing_folder = collection.add_folder(&target, "Version.1").unwrap();
    let existing_child = request(collection, &existing_folder, "Existing");
    let source_child = request(collection, &folder, "Source");
    let moved = collection.move_item(&folder, &MoveTarget::Inside(target.clone())).unwrap();
    assert_eq!(moved, target.join("Version.1 1"));
    assert!(existing_child.exists());
    assert!(moved.join(source_child.file_name().unwrap()).exists());
    assert!(!existing_folder.join(source_child.file_name().unwrap()).exists());
}

#[test]
fn invalid_moves_and_unchanged_order_do_not_modify_any_files() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let folder = collection.add_folder(&root, "Folder").unwrap();
    let child = collection.add_folder(&folder, "Child").unwrap();
    let nested = request(collection, &child, "Nested");
    let first = request(collection, &root, "First");
    let second = request(collection, &root, "Second");
    let original = snapshot(&root);

    for (source, target) in [
        (&folder, MoveTarget::Inside(folder.clone())),
        (&folder, MoveTarget::Inside(child.clone())),
        (&folder, MoveTarget::Before(nested.clone())),
        (&folder, MoveTarget::Before(child)),
        (&first, MoveTarget::Before(first.clone())),
        (&first, MoveTarget::Inside(second.clone())),
        (&first, MoveTarget::Inside(root.join("../outside"))),
        (&root, MoveTarget::Inside(folder.clone())),
    ] {
        assert!(!collection.can_move_item(source, &target));
        assert!(collection.move_item(source, &target).is_err());
    }
    collection.move_item(&first, &MoveTarget::Before(second.clone())).unwrap();
    collection.move_item(&second, &MoveTarget::Inside(root.clone())).unwrap();
    assert_eq!(snapshot(&root), original);
}

#[test]
fn folder_metadata_is_created_when_reordering_plain_directories() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let first = root.join("First");
    let second = root.join("Second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    collection.reload().unwrap();

    collection.move_item(&second, &MoveTarget::Before(first.clone())).unwrap();
    assert!(first.join("folder.yml").exists());
    assert!(second.join("folder.yml").exists());
    assert_eq!(Collection::open(&root).unwrap().items.iter().map(Item::path).collect::<Vec<_>>(), vec![second.as_path(), first.as_path()]);
}

#[test]
fn a_failed_move_restores_the_original_order_and_files() {
    let mut fixture = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let first = request(collection, &root, "First");
    let second = request(collection, &root, "Second");
    let folder = collection.add_folder(&root, "Target").unwrap();
    let broken = root.join("broken.yml");
    fs::write(&broken, "invalid: [").unwrap();
    let original = snapshot(&root);

    for target in [MoveTarget::Before(first.clone()), MoveTarget::Inside(folder.clone())] {
        assert!(collection.move_item(&second, &target).is_err());
        assert_eq!(snapshot(&root), original);
    }
    fs::remove_file(&broken).unwrap();
    collection.reload().unwrap();
    assert_eq!(collection.items.iter().map(Item::path).collect::<Vec<_>>(), vec![folder.as_path(), first.as_path(), second.as_path()]);
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_move_files_outside_the_collection_or_create_a_cycle() {
    use std::os::unix::fs::symlink;

    let mut fixture = Fixture::new();
    let outside = Fixture::new();
    let collection = &mut fixture.collection;
    let root = collection.root.clone();
    let source = request(collection, &root, "Source");
    let external = root.join("External");
    symlink(&outside.collection.root, &external).unwrap();
    collection.reload().unwrap();
    let original = snapshot(&outside.collection.root);

    assert!(collection.move_item(&source, &MoveTarget::Inside(external.clone())).is_err());
    assert!(source.exists());
    assert_eq!(snapshot(&outside.collection.root), original);
    fs::remove_file(external).unwrap();
}
