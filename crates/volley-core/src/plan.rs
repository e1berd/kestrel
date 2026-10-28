use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::collection::Collection;
use crate::format::{Auth, Defaults, KeyValue, RequestFile, Script};
use crate::session::Session;
use crate::vars::{Scope, Vars};

#[derive(Debug, Clone)]
pub struct Plan {
    pub path: PathBuf,
    pub name: String,
    pub collection_name: String,
    pub root: PathBuf,
    pub file: RequestFile,
    pub chain: Vec<(String, Defaults)>,
    pub env_name: Option<String>,
    pub env_vars: Vars,
    pub dotenv: Vars,
}
impl Plan {
    pub fn new(collection: &Collection, path: &Path, env: Option<&str>) -> Result<Self> {
        let request = collection.request(path).with_context(|| format!("no request at {}", path.display()))?;
        Self::with_file(collection, path, request.doc.data.clone(), env)
    }

    pub fn with_file(collection: &Collection, path: &Path, file: RequestFile, env: Option<&str>) -> Result<Self> {
        let mut chain = vec![("collection".to_string(), collection.defaults())];
        for folder in collection.ancestors(path) {
            chain.push((format!("folder {}", folder.name()), folder.defaults()));
        }
        let environment = env.and_then(|name| collection.environment(name));
        if let (Some(name), None) = (env, environment) {
            bail!("there is no environment `{name}`");
        }
        let name = if file.info.name.is_empty() { crate::collection::file_stem(path) } else { file.info.name.clone() };
        Ok(Self {
            path: path.to_path_buf(),
            name,
            collection_name: collection.name(),
            root: collection.root.clone(),
            file,
            chain,
            env_name: environment.map(|e| e.name()),
            env_vars: environment.map(|e| e.variables()).unwrap_or_default(),
            dotenv: collection.dotenv.clone(),
        })
    }

    pub fn auth(&self) -> Auth {
        let own = self.file.http.as_ref().and_then(|h| h.auth.clone()).unwrap_or_default();
        if own.mode() != "inherit" {
            return own;
        }
        for (_, defaults) in self.chain.iter().rev() {
            if let Some(auth) = &defaults.auth {
                if auth.mode() != "inherit" {
                    return auth.clone();
                }
            }
        }
        Auth::Mode("none".into())
    }

    pub fn scope(&self, session: &Session) -> Scope {
        let state = session.lock();
        let mut environment = self.env_vars.clone();
        if let Some(overrides) = self.env_name.as_ref().and_then(|name| state.env_overrides.get(name)) {
            environment.extend(overrides.clone());
        }
        let mut process_env: Vars = std::env::vars().collect();
        process_env.extend(self.dotenv.clone());
        Scope {
            collection: enabled(&self.chain[0].1.variables),
            environment,
            folders: self.chain[1..].iter().map(|(_, d)| enabled(&d.variables)).collect(),
            request: enabled(self.file.variables()),
            runtime: state.runtime_strings(),
            process_env,
        }
    }

    pub(crate) fn scripts(&self, kind: &str) -> Vec<(String, String)> {
        let pick = |label: &str, scripts: &[Script]| -> Vec<(String, String)> {
            scripts.iter().filter(|s| s.kind == kind).map(|s| (format!("{label} {kind} script"), s.code.clone())).collect()
        };
        let mut levels: Vec<Vec<(String, String)>> = self.chain.iter().map(|(label, d)| pick(label, &d.scripts)).collect();
        levels.push(pick("request", self.file.scripts()));
        if kind != "before-request" {
            levels.reverse();
        }
        levels.into_iter().flatten().collect()
    }
}
pub(crate) fn enabled(list: &[KeyValue]) -> Vars {
    list.iter().filter(|v| v.enabled()).map(|v| (v.name.clone(), v.value.clone())).collect()
}
