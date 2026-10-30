use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::collection::{Collection, Item};
use crate::loading::unique_path;
use crate::report::ReportExt as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoveTarget {
    Inside(PathBuf),
    Before(PathBuf),
    After(PathBuf),
}

impl Collection {
    pub fn can_move_item(&self, path: &Path, target: &MoveTarget) -> bool {
        self.move_destination(path, target).is_ok()
    }

    pub fn move_item(&mut self, path: &Path, target: &MoveTarget) -> Result<PathBuf> {
        let destination = self.move_destination(path, target)?;
        let source = self.item(path).context("the item is gone")?;
        let is_folder = matches!(source, Item::Folder(_));
        let siblings = self.move_siblings(&destination)?;
        let mut order: Vec<PathBuf> =
            siblings.iter().filter(|item| matches!(item, Item::Folder(_)) == is_folder).map(|item| item.path().to_path_buf()).collect();
        let previous = order.clone();
        order.retain(|sibling| sibling != path);
        let index = match target {
            MoveTarget::Inside(_) => order.len(),
            MoveTarget::Before(anchor) => order.iter().position(|sibling| sibling == anchor).context("the target is gone")?,
            MoveTarget::After(anchor) => order.iter().position(|sibling| sibling == anchor).context("the target is gone")? + 1,
        };
        order.insert(index, path.to_path_buf());
        if order == previous {
            return Ok(path.to_path_buf());
        }
        self.validate_move_paths(path, &destination)?;
        let new_path = if path.parent() == Some(destination.as_path()) {
            path.to_path_buf()
        } else {
            available_destination(path, &destination, is_folder)?
        };
        let updates: Vec<_> = order
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                let item = self.item(path).expect("the move order contains collection items");
                let sequence = index as i64 + 1;
                (item.seq() != sequence).then(|| (item.clone(), sequence, item_document_path(item).exists()))
            })
            .collect();
        for (index, (item, sequence, _)) in updates.iter().enumerate() {
            if let Err(error) = save_sequence(item.clone(), *sequence) {
                restore_documents(&updates[..=index]);
                return Err(error);
            }
        }
        if new_path != path
            && let Err(error) = fs::rename(path, &new_path)
        {
            restore_documents(&updates);
            return Err(error).with_context(|| format!("move {} to {}", path.display(), new_path.display()));
        }
        if let Err(error) = self.reload() {
            if new_path != path {
                fs::rename(&new_path, path).report_err("restore an item after a failed move");
            }
            restore_documents(&updates);
            return Err(error);
        }
        Ok(new_path)
    }

    fn move_destination(&self, path: &Path, target: &MoveTarget) -> Result<PathBuf> {
        let source = self.item(path).context("the item is gone")?;
        let destination = match target {
            MoveTarget::Inside(directory) => {
                self.move_siblings(directory)?;
                directory.clone()
            }
            MoveTarget::Before(anchor) | MoveTarget::After(anchor) => {
                let target = self.item(anchor).context("the target is gone")?;
                if path == anchor {
                    bail!("an item cannot be dropped onto itself");
                }
                if matches!(source, Item::Folder(_)) != matches!(target, Item::Folder(_)) {
                    bail!("folders and requests are ordered separately");
                }
                anchor.parent().context("the target has no parent")?.to_path_buf()
            }
        };
        if destination.starts_with(path) {
            bail!("a folder cannot be moved into itself or its descendants");
        }
        Ok(destination)
    }

    fn move_siblings(&self, directory: &Path) -> Result<&[Item]> {
        if directory == self.root {
            return Ok(&self.items);
        }
        match self.item(directory) {
            Some(Item::Folder(folder)) => Ok(&folder.children),
            _ => bail!("{} is not a collection folder", directory.display()),
        }
    }

    fn validate_move_paths(&self, path: &Path, destination: &Path) -> Result<()> {
        let source = path.canonicalize().context("resolve the item path")?;
        let destination = destination.canonicalize().context("resolve the destination folder")?;
        if !source.starts_with(&self.root) || !destination.starts_with(&self.root) || source == self.root {
            bail!("items can only be moved inside their collection");
        }
        if destination.starts_with(source) {
            bail!("a folder cannot be moved into itself or its descendants");
        }
        Ok(())
    }
}

fn available_destination(path: &Path, directory: &Path, is_folder: bool) -> Result<PathBuf> {
    let name = path.file_name().context("the item has no file name")?;
    let destination = directory.join(name);
    if !destination.try_exists()? {
        return Ok(destination);
    }
    let stem = if is_folder { name } else { path.file_stem().context("the request has no file stem")? };
    let extension = if is_folder { "" } else { path.extension().and_then(|extension| extension.to_str()).context("invalid extension")? };
    Ok(unique_path(directory, &stem.to_string_lossy(), extension))
}

fn item_document_path(item: &Item) -> &Path {
    match item {
        Item::Folder(folder) => &folder.doc.path,
        Item::Request(request) => &request.doc.path,
    }
}

fn save_sequence(mut item: Item, sequence: i64) -> Result<()> {
    match &mut item {
        Item::Folder(folder) => {
            folder.doc.data.info.seq = Some(sequence);
            folder.doc.save()
        }
        Item::Request(request) => {
            request.doc.data.info.seq = Some(sequence);
            request.doc.save()
        }
    }
}

fn restore_documents(updates: &[(Item, i64, bool)]) {
    for (item, _, existed) in updates {
        let restored = if *existed {
            match item.clone() {
                Item::Folder(mut folder) => folder.doc.save(),
                Item::Request(mut request) => request.doc.save(),
            }
        } else {
            match fs::remove_file(item_document_path(item)) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            }
        };
        restored.report_err("restore collection order after a failed move");
    }
}
