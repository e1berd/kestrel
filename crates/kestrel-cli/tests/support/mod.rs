pub mod server;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use kestrel_core::format::{Auth, Body, KeyValue};
use kestrel_core::{Collection, Item};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Case {
    pub path: String,
    pub version: String,
    pub title: String,
    pub requests: usize,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    cases: Vec<Case>,
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../kestrel-import/tests/fixtures")
}

pub fn cases() -> Vec<Case> {
    let text = std::fs::read_to_string(fixtures().join("upstream/oai/manifest.json")).unwrap();
    serde_json::from_str::<Manifest>(&text).unwrap().cases
}

pub struct Directory(pub PathBuf);

impl Directory {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kestrel-import-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kestrel-cli"));
        command.current_dir(&self.0).env("XDG_STATE_HOME", self.0.join("state")).env("NO_PROXY", "*");
        command
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove e2e files");
    }
}

pub fn succeed(command: &mut Command) -> Output {
    let output = command.output().expect("run kestrel-cli");
    assert!(
        output.status.success(),
        "{command:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(path.strip_prefix(root).unwrap().into(), std::fs::read(path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

pub fn assert_save_preserves_bytes(root: &Path) {
    fn save(items: &mut [Item]) {
        for item in items {
            match item {
                Item::Folder(folder) => {
                    folder.doc.save().unwrap();
                    save(&mut folder.children);
                }
                Item::Request(request) => request.doc.save().unwrap(),
            }
        }
    }
    let before = snapshot(root);
    let mut collection = Collection::open(root).unwrap();
    collection.doc.save().unwrap();
    save(&mut collection.items);
    assert_eq!(snapshot(root), before, "saving the imported collection changed its bytes");
}

pub fn edit_request(root: &Path) {
    let mut collection = Collection::open(root).unwrap();
    let path = collection.requests()[0].doc.path.clone();
    let request = collection.request_mut(&path).unwrap();
    let http = request.doc.data.http.as_mut().unwrap();
    http.headers.push(KeyValue::new("X-User-Edit", "preserve me"));
    http.auth = Some(Auth::Mode("none".into()));
    http.body = Some(Body::text("json", "{\"userEdit\":true}"));
    if let Some(parameter) = http.params.first_mut() {
        parameter.value = "custom-value".into();
        parameter.disabled = !parameter.disabled;
    }
    request.doc.save().unwrap();
}
