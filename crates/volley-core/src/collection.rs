use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use indexmap::IndexMap;

use crate::format::{CollectionFile, Defaults, EnvironmentFile, FolderFile, Info, RequestFile};
pub use crate::document::Document;
pub use crate::loading::{file_stem, sanitize_file_name};
use crate::loading::{find_root, read_dir, read_dotenv, read_environments, unique_path};

pub const COLLECTION_FILE: &str = "opencollection.yml";
pub const FOLDER_FILE: &str = "folder.yml";
pub const ENVIRONMENTS_DIR: &str = "environments";
#[derive(Debug, Clone)]
pub struct Folder {
    pub dir: PathBuf,
    pub doc: Document<FolderFile>,
    pub children: Vec<Item>,
}
impl Folder {
    pub fn name(&self) -> String {
        if self.doc.data.info.name.is_empty() { file_stem(&self.dir) } else { self.doc.data.info.name.clone() }
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
        if self.doc.data.info.name.is_empty() { file_stem(&self.doc.path) } else { self.doc.data.info.name.clone() }
    }

    pub fn method(&self) -> String {
        self.doc.data.http.as_ref().map(|h| h.method.to_uppercase()).unwrap_or_else(|| "GET".into())
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

    pub(crate) fn seq(&self) -> i64 {
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
        if self.doc.data.name.is_empty() { file_stem(&self.doc.path) } else { self.doc.data.name.clone() }
    }

    pub fn variables(&self) -> IndexMap<String, String> {
        self.doc.data.variables.iter().filter(|v| v.enabled()).map(|v| (v.name.clone(), v.value.clone())).collect()
    }
}
#[derive(Debug, Clone)]
pub struct Collection {
    pub root: PathBuf,
    pub doc: Document<CollectionFile>,
    pub items: Vec<Item>,
    pub environments: Vec<Environment>,
    pub dotenv: IndexMap<String, String>,
}
impl Collection {
    pub fn open(root: &Path) -> Result<Self> {
        let root =
            find_root(root).with_context(|| format!("{} is not a collection: there is no {COLLECTION_FILE} in it", root.display()))?;
        let doc = Document::<CollectionFile>::read(&root.join(COLLECTION_FILE), true)?;
        let mut ignored: HashSet<String> = doc.data.ignored().into_iter().collect();
        ignored.extend(["node_modules", ".git", ENVIRONMENTS_DIR].map(String::from));
        let items = read_dir(&root, &ignored)?;
        let environments = read_environments(&root.join(ENVIRONMENTS_DIR))?;
        let dotenv = read_dotenv(&root.join(".env"));
        Ok(Self { root, doc, items, environments, dotenv })
    }

    pub fn create(root: &Path, name: &str) -> Result<Self> {
        if root.join(COLLECTION_FILE).exists() {
            bail!("{} already holds a collection", root.display());
        }
        fs::create_dir_all(root)?;
        let mut doc = Document::new(
            root.join(COLLECTION_FILE),
            CollectionFile {
                opencollection: Some("1.0.0".into()),
                info: Info { name: name.into(), ..Default::default() },
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
        if self.doc.data.info.name.is_empty() { file_stem(&self.root) } else { self.doc.data.info.name.clone() }
    }

    pub fn defaults(&self) -> Defaults {
        self.doc.data.request.clone().unwrap_or_default()
    }

    pub fn environment(&self, name: &str) -> Option<&Environment> {
        self.environments.iter().find(|env| env.name() == name || file_stem(&env.doc.path) == name)
    }

    pub fn environment_mut(&mut self, name: &str) -> Option<&mut Environment> {
        self.environments.iter_mut().find(|env| env.name() == name || file_stem(&env.doc.path) == name)
    }

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
        let info = Info { name: name.into(), kind: "folder".into(), seq: Some(self.next_seq(parent)), ..Default::default() };
        let mut doc = Document::new(dir.join(FOLDER_FILE), FolderFile { info, ..Default::default() }, true);
        doc.save()?;
        self.reload()?;
        Ok(dir)
    }

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

    pub fn rename(&mut self, path: &Path, name: &str) -> Result<PathBuf> {
        let parent = path.parent().context("no parent")?.to_path_buf();
        let new_path = match self.item_mut(path).context("no such item")? {
            Item::Request(request) => {
                request.doc.data.info.name = name.into();
                let new_path = unique_path(&parent, &sanitize_file_name(name), "yml");
                let new_path = if file_stem(path) == sanitize_file_name(name) { path.to_path_buf() } else { new_path };
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
        siblings.iter().map(Item::seq).filter(|s| *s != i64::MAX).max().unwrap_or(0) + 1
    }
}
