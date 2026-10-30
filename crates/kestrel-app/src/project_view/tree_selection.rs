use std::path::{Path, PathBuf};

use kestrel_core::{Collection, Item};

#[derive(Default)]
pub(super) struct TreeSelection {
    path: Option<PathBuf>,
}

impl TreeSelection {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn select(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
    }

    pub fn directory(&self, collection: &Collection) -> PathBuf {
        match self.path().and_then(|path| collection.item(path)) {
            Some(Item::Folder(folder)) => folder.dir.clone(),
            Some(Item::Request(request)) => request.doc.path.parent().unwrap_or(&collection.root).to_path_buf(),
            None => collection.root.clone(),
        }
    }

    pub fn relocate(&mut self, old: &Path, new: &Path) {
        if let Some(path) = &self.path
            && let Ok(relative) = path.strip_prefix(old)
        {
            self.path = Some(new.join(relative));
        }
    }

    pub fn retain_existing(&mut self, collection: &Collection) {
        if self.path().is_some_and(|path| path != collection.root && collection.item(path).is_none()) {
            self.path = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use kestrel_core::format::{CollectionFile, FolderFile, RequestFile};
    use kestrel_core::{Document, Folder, Request};

    use super::*;

    fn collection() -> Collection {
        let root = PathBuf::from("/collection");
        let folder = root.join("folder");
        let nested = folder.join("nested");
        Collection {
            doc: Document::new(root.join("opencollection.yml"), CollectionFile::default(), true),
            items: vec![Item::Folder(Folder {
                doc: Document::new(folder.join("folder.yml"), FolderFile::default(), true),
                dir: folder,
                children: vec![Item::Folder(Folder {
                    doc: Document::new(nested.join("folder.yml"), FolderFile::default(), true),
                    dir: nested.clone(),
                    children: vec![Item::Request(Request {
                        doc: Document::new(nested.join("request.yml"), RequestFile::new_http("Request", "GET", "/"), true),
                    })],
                })],
            })],
            root,
            environments: Vec::new(),
            dotenv: Default::default(),
        }
    }

    #[test]
    fn creation_uses_the_selected_folder_or_request_parent() {
        let collection = collection();
        let mut selection = TreeSelection::default();
        assert_eq!(selection.directory(&collection), collection.root);
        for path in ["folder", "folder/nested", "folder/nested/request.yml"] {
            selection.select(&collection.root.join(path));
            let expected = if path.ends_with(".yml") { "folder/nested" } else { path };
            assert_eq!(selection.directory(&collection), collection.root.join(expected));
        }
        selection.select(&collection.root);
        assert_eq!(selection.directory(&collection), collection.root);
    }

    #[test]
    fn moving_or_renaming_a_folder_keeps_its_selected_descendant() {
        let mut selection = TreeSelection::default();
        selection.select(Path::new("/collection/folder/nested/request.yml"));
        selection.relocate(Path::new("/collection/other"), Path::new("/collection/unrelated"));
        assert_eq!(selection.path(), Some(Path::new("/collection/folder/nested/request.yml")));
        selection.relocate(Path::new("/collection/folder"), Path::new("/collection/renamed"));
        assert_eq!(selection.path(), Some(Path::new("/collection/renamed/nested/request.yml")));
    }

    #[test]
    fn deleted_selection_falls_back_to_the_collection_root() {
        let mut collection = collection();
        let mut selection = TreeSelection::default();
        selection.select(&collection.root.join("folder/nested"));
        collection.items.clear();
        assert_eq!(selection.directory(&collection), collection.root);
        selection.retain_existing(&collection);
        assert_eq!(selection.path(), None);
        selection.select(&collection.root);
        selection.retain_existing(&collection);
        assert_eq!(selection.path(), Some(collection.root.as_path()));
    }
}
