use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_yaml_ng::Value;

use crate::yaml::{self, Layout};

#[derive(Debug, Clone)]
pub struct Document<T> {
    pub path: PathBuf,
    pub data: T,
    source: Option<Arc<str>>,
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
            serde_yaml_ng::from_value(raw).with_context(|| format!("read {}", path.display()))?
        };
        Ok(Self { path: path.to_path_buf(), data, source: Some(text.into()), layout: Layout::of(text, space_keys) })
    }

    pub fn new(path: PathBuf, data: T, space_keys: bool) -> Self {
        Self { path, data, source: None, layout: Layout::fresh(space_keys) }
    }

    pub fn render(&self) -> Result<String> {
        let raw = self.original()?;
        let new = serde_yaml_ng::to_value(&self.data)?;
        let merged = yaml::merge(&raw, new);
        Ok(yaml::emit(&merged, &self.layout, &yaml::top_keys(&raw)))
    }

    pub fn is_dirty(&self) -> bool {
        match self.original().and_then(|raw| Ok(yaml::merge(&raw, serde_yaml_ng::to_value(&self.data)?) != raw)) {
            Ok(dirty) => dirty,
            Err(error) => {
                crate::report::report_error("compare a document with its saved contents", error);
                true
            }
        }
    }

    pub fn save(&mut self) -> Result<()> {
        let text = self.render()?;
        serde_yaml_ng::from_str::<Value>(&text)?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, &text).with_context(|| format!("write {}", self.path.display()))?;
        self.layout = Layout::of(&text, self.layout.space_new_keys);
        self.source = Some(text.into());
        Ok(())
    }

    pub fn revert(&mut self) {
        match self.original().and_then(|raw| Ok(serde_yaml_ng::from_value(raw)?)) {
            Ok(data) => self.data = data,
            Err(error) => crate::report::report_error("revert a document to its saved contents", error),
        }
    }

    fn original(&self) -> Result<Value> {
        match self.source.as_deref() {
            Some(text) if !text.trim().is_empty() => {
                serde_yaml_ng::from_str(text).with_context(|| format!("parse {}", self.path.display()))
            }
            _ => Ok(Value::Null),
        }
    }
}
