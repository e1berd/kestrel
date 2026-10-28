//! What the app remembers between runs: the collections opened lately, the environment each one
//! used and the theme. Kept in `~/.config/kestrel/prefs.json`, never in a collection.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

const RECENT_LIMIT: usize = 10;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    #[serde(default)]
    pub envs: IndexMap<String, String>,
    #[serde(default)]
    pub dark: Option<bool>,
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("kestrel").join("prefs.json"))
    }

    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Changes the stored prefs and writes them; a failed write only loses the change.
    pub fn update(change: impl FnOnce(&mut Prefs)) {
        let mut prefs = Self::load();
        change(&mut prefs);
        if let Some(path) = Self::path() {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
            let _ = std::fs::write(path, serde_json::to_string_pretty(&prefs).unwrap_or_default());
        }
    }

    pub fn opened(&mut self, root: &Path) {
        self.recent.retain(|p| p != root);
        self.recent.insert(0, root.to_path_buf());
        self.recent.truncate(RECENT_LIMIT);
    }

    pub fn env_for(&self, root: &Path) -> Option<String> {
        self.envs.get(&root.display().to_string()).cloned()
    }

    pub fn set_env(&mut self, root: &Path, env: Option<String>) {
        let key = root.display().to_string();
        match env {
            Some(env) => {
                self.envs.insert(key, env);
            }
            None => {
                self.envs.shift_remove(&key);
            }
        }
    }
}
