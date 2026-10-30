use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_yaml_ng::Value;

use crate::yaml::{self, Layout};

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
            serde_yaml_ng::from_value(raw.clone()).with_context(|| format!("read {}", path.display()))?
        };
        Ok(Self { path: path.to_path_buf(), data, raw, layout: Layout::of(text, space_keys) })
    }

    pub fn new(path: PathBuf, data: T, space_keys: bool) -> Self {
        Self { path, data, raw: Value::Null, layout: Layout::fresh(space_keys) }
    }

    pub fn render(&self) -> Result<String> {
        let new = serde_yaml_ng::to_value(&self.data)?;
        let merged = yaml::merge(&self.raw, new);
        Ok(yaml::emit(&merged, &self.layout, &yaml::top_keys(&self.raw)))
    }

    pub fn is_dirty(&self) -> bool {
        serde_yaml_ng::to_value(&self.data).map(|new| yaml::merge(&self.raw, new) != self.raw).unwrap_or(true)
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

    pub fn revert(&mut self) {
        if let Ok(data) = serde_yaml_ng::from_value(self.raw.clone()) {
            self.data = data;
        }
    }
}
