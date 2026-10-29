pub(crate) mod documents;
mod seed;
mod yaml;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead as _, BufReader};
use std::path::Path;

use anyhow::{Context as _, Result};
use serde::de::DeserializeSeed as _;
use serde_json::{Value as Json, json};
use volley_core::transfer::{Progress, Unit};

use crate::storage::{Location, Store, TemporaryDirectory};

pub(crate) struct Input {
    pub metadata: Json,
    pub nodes: BTreeMap<String, Location>,
    pub paths: Vec<String>,
    pub requests: Vec<(Option<usize>, Location)>,
    pub folders: Vec<Option<(Option<usize>, Json)>>,
    pub store: Store,
    _directory: TemporaryDirectory,
}

impl Input {
    pub fn read(path: &Path, progress: &Progress) -> Result<Self> {
        let directory = TemporaryDirectory::new()?;
        let mut input = Self {
            metadata: json!({}),
            nodes: BTreeMap::new(),
            paths: Vec::new(),
            requests: Vec::new(),
            folders: Vec::new(),
            store: Store::new(&directory.0.join("nodes"))?,
            _directory: directory,
        };
        let file = File::open(path).with_context(|| format!("read {}", path.display()))?;
        progress.start("Reading input", Some(file.metadata()?.len()), Unit::Bytes)?;
        let mut reader = BufReader::with_capacity(64 * 1024, progress.reader(file));
        if reader.fill_buf()?.starts_with(&[0xef, 0xbb, 0xbf]) {
            reader.consume(3);
        }
        while reader.fill_buf()?.first().is_some_and(u8::is_ascii_whitespace) {
            reader.consume(1);
        }
        if reader.fill_buf()?.first() == Some(&b'{') {
            let mut deserializer = serde_json::Deserializer::from_reader(reader);
            seed::Root(&mut input).deserialize(&mut deserializer)?;
            deserializer.end()?;
        } else {
            let translated = input._directory.0.join("input.json");
            yaml::translate(reader, &translated)?;
            let file = File::open(&translated)?;
            progress.start("Indexing input", Some(file.metadata()?.len()), Unit::Bytes)?;
            let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(progress.reader(file)));
            seed::Root(&mut input).deserialize(&mut deserializer)?;
            deserializer.end()?;
        }
        Ok(input)
    }

    pub fn node(&mut self, pointer: &str) -> Result<Option<Json>> {
        if pointer.is_empty() {
            let mut root = self.metadata.clone();
            let entries: Vec<_> = self
                .nodes
                .iter()
                .filter(|(pointer, _)| !pointer[1..].contains('/'))
                .map(|(pointer, location)| (pointer.clone(), *location))
                .collect();
            for (pointer, location) in entries {
                root[pointer[1..].replace("~1", "/").replace("~0", "~")] = self.store.read::<Json>(location)?;
            }
            return Ok(Some(root));
        }
        let mut prefix = pointer;
        loop {
            if let Some(location) = self.nodes.get(prefix).copied() {
                let value: Json = self.store.read(location)?;
                if pointer.len() == prefix.len() {
                    return Ok(Some(value));
                }
                return Ok(value.pointer(&pointer[prefix.len()..]).cloned());
            }
            let Some((parent, _)) = prefix.rsplit_once('/') else { break };
            prefix = parent;
        }
        Ok(self.metadata.pointer(pointer).cloned())
    }

    pub fn folder_chain(&self, mut parent: Option<usize>) -> Vec<&Json> {
        let mut folders = Vec::new();
        while let Some(index) = parent {
            if let Some((next, metadata)) = &self.folders[index] {
                folders.push(metadata);
                parent = *next;
            } else {
                break;
            }
        }
        folders.reverse();
        folders
    }
}

pub(crate) fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
