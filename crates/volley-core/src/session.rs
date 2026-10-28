//! What lasts between sends while the app is open or a run goes on: variables scripts set, tokens
//! OAuth2 handed out and cookies servers set.

use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use reqwest::cookie::Jar;
use serde_json::Value as Json;

use crate::oauth2::Token;
use crate::vars::Vars;

#[derive(Default)]
pub struct SessionState {
    /// `bru.setVar`: any JSON value, as scripts hold it.
    pub runtime: IndexMap<String, Json>,
    /// `bru.setEnvVar`, by environment; kept in memory, the file is not touched.
    pub env_overrides: IndexMap<String, Vars>,
    /// OAuth2 tokens by their `tokenConfig.id`.
    pub tokens: IndexMap<String, Token>,
}

impl SessionState {
    pub fn runtime_strings(&self) -> Vars {
        self.runtime
            .iter()
            .map(|(k, v)| (k.clone(), json_text(v)))
            .collect()
    }
}

/// The text a JSON value stands for in `{{name}}`.
pub fn json_text(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

#[derive(Clone, Default)]
pub struct Session {
    state: Arc<Mutex<SessionState>>,
    pub cookies: Arc<Jar>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lock(&self) -> MutexGuard<'_, SessionState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn clear(&self) {
        *self.lock() = SessionState::default();
    }
}
