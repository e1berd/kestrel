use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::collection::{Collection, Item};
use crate::format::KeyValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableHome {
    Collection,
    Folder(PathBuf),
    Environment(String),
}

impl VariableHome {
    pub fn describe(&self) -> String {
        match self {
            VariableHome::Collection => "collection variables (opencollection.yml)".into(),
            VariableHome::Folder(dir) => format!("folder {} (folder.yml)", crate::collection::file_stem(dir)),
            VariableHome::Environment(name) => format!("environment {name}"),
        }
    }
}

impl Collection {
    pub fn set_variable(&mut self, home: &VariableHome, name: &str, value: &str) -> Result<()> {
        match home {
            VariableHome::Collection => {
                upsert(&mut self.doc.data.request.get_or_insert_with(Default::default).variables, name, value);
                self.doc.save()
            }
            VariableHome::Folder(dir) => {
                let Some(Item::Folder(folder)) = self.item_mut(dir) else { return Err(no_folder(dir)) };
                upsert(&mut folder.doc.data.request.get_or_insert_with(Default::default).variables, name, value);
                folder.doc.save()
            }
            VariableHome::Environment(env) => {
                let environment = self.environment_mut(env).with_context(|| format!("there is no environment `{env}`"))?;
                upsert(&mut environment.doc.data.variables, name, value);
                environment.doc.save()
            }
        }
    }
}

fn upsert(variables: &mut Vec<KeyValue>, name: &str, value: &str) {
    match variables.iter_mut().find(|variable| variable.enabled() && variable.name == name) {
        Some(variable) => variable.value = value.to_string(),
        None => variables.push(KeyValue { name: name.to_string(), value: value.to_string(), ..Default::default() }),
    }
}

fn no_folder(dir: &Path) -> anyhow::Error {
    anyhow::anyhow!("there is no folder at {}", dir.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_collection(name: &str) -> Collection {
        let root = std::env::temp_dir().join(format!("kestrel-variable-home-{name}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("clear the scratch collection");
        }
        Collection::create(&root, name).expect("create a collection")
    }

    #[test]
    fn a_new_collection_variable_is_written_and_read_back() {
        let mut collection = scratch_collection("collection");
        collection.set_variable(&VariableHome::Collection, "baseUrl", "https://example.com").unwrap();
        collection.set_variable(&VariableHome::Collection, "baseUrl", "https://example.org").unwrap();
        let reopened = Collection::open(&collection.root).unwrap();
        let variables = reopened.defaults().variables;
        assert_eq!(variables.len(), 1);
        assert_eq!(variables[0].value, "https://example.org");
    }

    #[test]
    fn a_missing_environment_is_an_error() {
        let mut collection = scratch_collection("environment");
        let home = VariableHome::Environment("staging".into());
        assert!(collection.set_variable(&home, "baseUrl", "x").is_err());
    }
}
