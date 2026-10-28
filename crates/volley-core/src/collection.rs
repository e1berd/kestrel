//! A collection on disk: a folder tree of YAML files that lives in git like any other code.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_yaml_ng::Value;

use crate::format::{CollectionFile, Defaults, EnvironmentFile, FolderFile, Info, RequestFile};
use crate::yaml::{self, Layout};

pub const COLLECTION_FILE: &str = "opencollection.yml";
pub const FOLDER_FILE: &str = "folder.yml";
pub const ENVIRONMENTS_DIR: &str = "environments";

/// A YAML file together with what it looked like when read, so saving keeps its key order and
/// spacing and rewrites only what changed.
#[derive(Debug, Clone)]
pub struct Document<T> {
    pub path: PathBuf,
    pub data: T,
    raw: Value,
    layout: Layout,
}

impl<T: Serialize + DeserializeOwned + Default> Document<T> {
    pub fn read(path: &Path, space_keys: bool) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::parse(path, &text, space_keys)
    }

    pub fn parse(path: &Path, text: &str, space_keys: bool) -> Result<Self> {
        let raw: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_yaml_ng::from_str(text).with_context(|| format!("parse {}", path.display()))?
        };
        let data = if raw.is_null() {
            T::default()
        } else {
            serde_yaml_ng::from_value(raw.clone())
                .with_context(|| format!("read {}", path.display()))?
        };
        Ok(Self {
            path: path.to_path_buf(),
            data,
            raw,
            layout: Layout::of(text, space_keys),
        })
    }

    pub fn new(path: PathBuf, data: T, space_keys: bool) -> Self {
        Self {
            path,
            data,
            raw: Value::Null,
            layout: Layout::fresh(space_keys),
        }
    }

    /// The file's text as it would be written now.
    pub fn render(&self) -> Result<String> {
        let new = serde_yaml_ng::to_value(&self.data)?;
        let merged = yaml::merge(&self.raw, new);
        Ok(yaml::emit(
            &merged,
            &self.layout,
            &yaml::top_keys(&self.raw),
        ))
    }

    /// Whether `data` differs from what was read.
    pub fn is_dirty(&self) -> bool {
        serde_yaml_ng::to_value(&self.data)
            .map(|new| yaml::merge(&self.raw, new) != self.raw)
            .unwrap_or(true)
    }

    pub fn save(&mut self) -> Result<()> {
        let text = self.render()?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, &text).with_context(|| format!("write {}", self.path.display()))?;
        self.raw = serde_yaml_ng::from_str(&text)?;
        self.layout = Layout::of(&text, self.layout.space_new_keys);
        Ok(())
    }

    /// Back to what is on disk.
    pub fn revert(&mut self) {
        if let Ok(data) = serde_yaml_ng::from_value(self.raw.clone()) {
            self.data = data;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Folder {
    pub dir: PathBuf,
    pub doc: Document<FolderFile>,
    pub children: Vec<Item>,
}

impl Folder {
    pub fn name(&self) -> String {
        if self.doc.data.info.name.is_empty() {
            file_stem(&self.dir)
        } else {
            self.doc.data.info.name.clone()
        }
    }

    pub fn defaults(&self) -> Defaults {
        self.doc.data.request.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub doc: Document<RequestFile>,
}

impl Request {
    pub fn name(&self) -> String {
        if self.doc.data.info.name.is_empty() {
            file_stem(&self.doc.path)
        } else {
            self.doc.data.info.name.clone()
        }
    }

    pub fn method(&self) -> String {
        self.doc
            .data
            .http
            .as_ref()
            .map(|h| h.method.to_uppercase())
            .unwrap_or_else(|| "GET".into())
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    Folder(Folder),
    Request(Request),
}

impl Item {
    pub fn path(&self) -> &Path {
        match self {
            Item::Folder(folder) => &folder.dir,
            Item::Request(request) => &request.doc.path,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Item::Folder(folder) => folder.name(),
            Item::Request(request) => request.name(),
        }
    }

    fn seq(&self) -> i64 {
        let seq = match self {
            Item::Folder(folder) => folder.doc.data.info.seq,
            Item::Request(request) => request.doc.data.info.seq,
        };
        seq.unwrap_or(i64::MAX)
    }
}

#[derive(Debug, Clone)]
pub struct Environment {
    pub doc: Document<EnvironmentFile>,
}

impl Environment {
    pub fn name(&self) -> String {
        if self.doc.data.name.is_empty() {
            file_stem(&self.doc.path)
        } else {
            self.doc.data.name.clone()
        }
    }

    /// Its enabled variables, in order.
    pub fn variables(&self) -> IndexMap<String, String> {
        self.doc
            .data
            .variables
            .iter()
            .filter(|v| v.enabled())
            .map(|v| (v.name.clone(), v.value.clone()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Collection {
    pub root: PathBuf,
    pub doc: Document<CollectionFile>,
    pub items: Vec<Item>,
    pub environments: Vec<Environment>,
    /// The collection's `.env`, which stays out of git and is read by `{{process.env.NAME}}`.
    pub dotenv: IndexMap<String, String>,
}

impl Collection {
    /// Reads the collection whose root is `root`, or the folder `root` sits in.
    pub fn open(root: &Path) -> Result<Self> {
        let root = find_root(root).with_context(|| {
            format!(
                "{} is not a collection: there is no {COLLECTION_FILE} in it",
                root.display()
            )
        })?;
        let doc = Document::<CollectionFile>::read(&root.join(COLLECTION_FILE), true)?;
        let mut ignored: HashSet<String> = doc.data.ignored().into_iter().collect();
        ignored.extend(["node_modules", ".git", ENVIRONMENTS_DIR].map(String::from));
        let items = read_dir(&root, &ignored)?;
        let environments = read_environments(&root.join(ENVIRONMENTS_DIR))?;
        let dotenv = read_dotenv(&root.join(".env"));
        Ok(Self {
            root,
            doc,
            items,
            environments,
            dotenv,
        })
    }

    /// Makes a new collection in `root`, which must not hold one already.
    pub fn create(root: &Path, name: &str) -> Result<Self> {
        if root.join(COLLECTION_FILE).exists() {
            bail!("{} already holds a collection", root.display());
        }
        fs::create_dir_all(root)?;
        let mut doc = Document::new(
            root.join(COLLECTION_FILE),
            CollectionFile {
                opencollection: Some("1.0.0".into()),
                info: Info {
                    name: name.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            true,
        );
        doc.save()?;
        let gitignore = root.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, ".env\n")?;
        }
        Self::open(root)
    }

    pub fn name(&self) -> String {
        if self.doc.data.info.name.is_empty() {
            file_stem(&self.root)
        } else {
            self.doc.data.info.name.clone()
        }
    }

    pub fn defaults(&self) -> Defaults {
        self.doc.data.request.clone().unwrap_or_default()
    }

    pub fn environment(&self, name: &str) -> Option<&Environment> {
        self.environments
            .iter()
            .find(|env| env.name() == name || file_stem(&env.doc.path) == name)
    }

    pub fn environment_mut(&mut self, name: &str) -> Option<&mut Environment> {
        self.environments
            .iter_mut()
            .find(|env| env.name() == name || file_stem(&env.doc.path) == name)
    }

    /// Every request, depth first in the order the tree shows them.
    pub fn requests(&self) -> Vec<&Request> {
        fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Request>) {
            for item in items {
                match item {
                    Item::Folder(folder) => walk(&folder.children, out),
                    Item::Request(request) => out.push(request),
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.items, &mut out);
        out
    }

    pub fn item(&self, path: &Path) -> Option<&Item> {
        fn find<'a>(items: &'a [Item], path: &Path) -> Option<&'a Item> {
            for item in items {
                if item.path() == path {
                    return Some(item);
                }
                if let Item::Folder(folder) = item {
                    if path.starts_with(&folder.dir) {
                        return find(&folder.children, path);
                    }
                }
            }
            None
        }
        find(&self.items, path)
    }

    pub fn item_mut(&mut self, path: &Path) -> Option<&mut Item> {
        fn find<'a>(items: &'a mut [Item], path: &Path) -> Option<&'a mut Item> {
            for item in items {
                if item.path() == path {
                    return Some(item);
                }
                if let Item::Folder(folder) = item {
                    if path.starts_with(&folder.dir) {
                        return find(&mut folder.children, path);
                    }
                }
            }
            None
        }
        find(&mut self.items, path)
    }

    pub fn request(&self, path: &Path) -> Option<&Request> {
        match self.item(path)? {
            Item::Request(request) => Some(request),
            Item::Folder(_) => None,
        }
    }

    pub fn request_mut(&mut self, path: &Path) -> Option<&mut Request> {
        match self.item_mut(path)? {
            Item::Request(request) => Some(request),
            Item::Folder(_) => None,
        }
    }

    /// The folders from the root down to the one that holds `path`.
    pub fn ancestors(&self, path: &Path) -> Vec<&Folder> {
        let mut out = Vec::new();
        let mut items = &self.items;
        'down: loop {
            for item in items {
                if let Item::Folder(folder) = item {
                    if path.starts_with(&folder.dir) && path != folder.dir {
                        out.push(folder);
                        items = &folder.children;
                        continue 'down;
                    }
                }
            }
            return out;
        }
    }

    /// Writes a new request file into `dir` and adds it to the tree.
    pub fn add_request(&mut self, dir: &Path, file: RequestFile) -> Result<PathBuf> {
        let path = unique_path(dir, &sanitize_file_name(&file.info.name), "yml");
        let mut file = file;
        if file.info.seq.is_none() {
            file.info.seq = Some(self.next_seq(dir));
        }
        let mut doc = Document::new(path.clone(), file, true);
        doc.save()?;
        self.reload()?;
        Ok(path)
    }

    pub fn add_folder(&mut self, parent: &Path, name: &str) -> Result<PathBuf> {
        let dir = unique_path(parent, &sanitize_file_name(name), "");
        fs::create_dir_all(&dir)?;
        let info = Info {
            name: name.into(),
            kind: "folder".into(),
            seq: Some(self.next_seq(parent)),
            ..Default::default()
        };
        let mut doc = Document::new(
            dir.join(FOLDER_FILE),
            FolderFile {
                info,
                ..Default::default()
            },
            true,
        );
        doc.save()?;
        self.reload()?;
        Ok(dir)
    }

    /// Deletes a request file or a whole folder from disk.
    pub fn remove(&mut self, path: &Path) -> Result<()> {
        if !path.starts_with(&self.root) || path == self.root {
            bail!("{} is not inside the collection", path.display());
        }
        if path.is_dir() {
            fs::remove_dir_all(path)?
        } else {
            fs::remove_file(path)?
        }
        self.reload()
    }

    /// Renames a request (and its file) or a folder (and its directory).
    pub fn rename(&mut self, path: &Path, name: &str) -> Result<PathBuf> {
        let parent = path.parent().context("no parent")?.to_path_buf();
        let new_path = match self.item_mut(path).context("no such item")? {
            Item::Request(request) => {
                request.doc.data.info.name = name.into();
                let new_path = unique_path(&parent, &sanitize_file_name(name), "yml");
                let new_path = if file_stem(path) == sanitize_file_name(name) {
                    path.to_path_buf()
                } else {
                    new_path
                };
                request.doc.save()?;
                new_path
            }
            Item::Folder(folder) => {
                folder.doc.data.info.name = name.into();
                if folder.doc.data.info.kind.is_empty() {
                    folder.doc.data.info.kind = "folder".into();
                }
                folder.doc.save()?;
                if file_stem(path) == sanitize_file_name(name) {
                    path.to_path_buf()
                } else {
                    unique_path(&parent, &sanitize_file_name(name), "")
                }
            }
        };
        if new_path != path {
            fs::rename(path, &new_path)?;
        }
        self.reload()?;
        Ok(new_path)
    }

    /// Reads the tree, the environments and `.env` again, for when git or an editor changed them.
    pub fn reload(&mut self) -> Result<()> {
        *self = Self::open(&self.root)?;
        Ok(())
    }

    fn next_seq(&self, dir: &Path) -> i64 {
        let siblings: &[Item] = if dir == self.root {
            &self.items
        } else {
            match self.item(dir) {
                Some(Item::Folder(folder)) => &folder.children,
                _ => &[],
            }
        };
        siblings
            .iter()
            .map(Item::seq)
            .filter(|s| *s != i64::MAX)
            .max()
            .unwrap_or(0)
            + 1
    }
}

fn find_root(start: &Path) -> Option<PathBuf> {
    let start = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    let start = start.canonicalize().ok()?;
    start
        .ancestors()
        .find(|dir| dir.join(COLLECTION_FILE).is_file())
        .map(Path::to_path_buf)
}

fn read_dir(dir: &Path, ignored: &HashSet<String>) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') || ignored.contains(&name) {
            continue;
        }
        if path.is_dir() {
            let folder_file = path.join(FOLDER_FILE);
            let doc = if folder_file.is_file() {
                Document::read(&folder_file, true)?
            } else {
                Document::new(folder_file, FolderFile::default(), true)
            };
            let children = read_dir(&path, ignored)?;
            items.push(Item::Folder(Folder {
                dir: path,
                doc,
                children,
            }));
        } else if is_request_file(&path) {
            let doc = Document::<RequestFile>::read(&path, true)?;
            if doc.data.info.kind.is_empty()
                || doc.data.info.kind == "http"
                || doc.data.http.is_some()
            {
                items.push(Item::Request(Request { doc }));
            }
        }
    }
    // Folders first, each part in the order its files ask for.
    items.sort_by(|a, b| {
        let folder = |item: &Item| matches!(item, Item::Folder(_));
        folder(b)
            .cmp(&folder(a))
            .then(a.seq().cmp(&b.seq()))
            .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
    });
    Ok(items)
}

fn is_request_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yml" | "yaml")
    ) && name != FOLDER_FILE
        && name != COLLECTION_FILE
}

fn read_environments(dir: &Path) -> Result<Vec<Environment>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            Ok(Environment {
                doc: Document::read(&path, false)?,
            })
        })
        .collect()
}

fn read_dotenv(path: &Path) -> IndexMap<String, String> {
    match dotenvy::from_path_iter(path) {
        Ok(iter) => iter.filter_map(|e| e.ok()).collect(),
        Err(_) => IndexMap::new(),
    }
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// A request's name as a file name: what the file system forbids becomes `-`, the way Bruno does it.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "untitled".into()
    } else {
        cleaned
    }
}

fn unique_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let make = |n: usize| {
        let stem = if n == 0 {
            stem.to_string()
        } else {
            format!("{stem} {n}")
        };
        if extension.is_empty() {
            dir.join(stem)
        } else {
            dir.join(format!("{stem}.{extension}"))
        }
    };
    (0..).map(make).find(|p| !p.exists()).expect("a free name")
}
