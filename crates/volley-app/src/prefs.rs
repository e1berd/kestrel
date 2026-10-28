use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use volley_core::report::report_error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterfaceFont {
    #[default]
    Inter,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeChoice,
    pub scheme: String,
    pub interface_scale: u32,
    pub text_scale: u32,
    pub code_font_size: u32,
    pub is_low_vision: bool,
    pub interface_font: InterfaceFont,
    pub open_projects: Vec<PathBuf>,
    pub active_project: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub envs: IndexMap<String, String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            scheme: crate::appearance::palette::DEFAULT_SCHEME.into(),
            interface_scale: 100,
            text_scale: 100,
            code_font_size: 13,
            is_low_vision: false,
            interface_font: InterfaceFont::Inter,
            open_projects: Vec::new(),
            active_project: None,
            recent: Vec::new(),
            envs: IndexMap::new(),
        }
    }
}

const RECENT_LIMIT: usize = 12;

impl Prefs {
    pub fn load() -> Self {
        cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    pub fn update(change: impl FnOnce(&mut Prefs)) {
        let snapshot = {
            let mut cached = cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            change(&mut cached);
            cached.clone()
        };
        write(&snapshot);
    }

    pub fn opened(&mut self, root: &Path) {
        self.recent.retain(|path| path != root);
        self.recent.insert(0, root.to_path_buf());
        self.recent.truncate(RECENT_LIMIT);
        if !self.open_projects.iter().any(|path| path == root) {
            self.open_projects.push(root.to_path_buf());
        }
        self.active_project = Some(root.to_path_buf());
    }

    pub fn closed(&mut self, root: &Path) {
        self.open_projects.retain(|path| path != root);
        if self.active_project.as_deref() == Some(root) {
            self.active_project = self.open_projects.last().cloned();
        }
    }

    pub fn env_for(&self, root: &Path) -> Option<String> {
        self.envs.get(&root.display().to_string()).cloned()
    }

    pub fn set_env(&mut self, root: &Path, env: Option<String>) {
        let key = root.display().to_string();
        match env {
            Some(env) => self.envs.insert(key, env),
            None => self.envs.shift_remove(&key),
        };
    }
}

fn cache() -> &'static Mutex<Prefs> {
    static CACHE: OnceLock<Mutex<Prefs>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(read()))
}

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("kestrel").join("prefs.json"))
}

fn read() -> Prefs {
    let Some(path) = path() else { return Prefs::default() };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            report_error("read the preferences", error);
            Prefs::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Prefs::default(),
        Err(error) => {
            report_error("read the preferences", error);
            Prefs::default()
        }
    }
}

fn write(prefs: &Prefs) {
    let Some(path) = path() else { return };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, serde_json::to_vec_pretty(prefs).unwrap_or_default()));
    if let Err(error) = written {
        report_error("save the preferences", error);
    }
}
