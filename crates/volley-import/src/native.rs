use indexmap::IndexMap;
use volley_core::format::{Docs, KeyValue, RequestFile};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Request {
    pub folders: Vec<String>,
    pub file: RequestFile,
}

#[derive(Debug, Clone)]
pub(super) struct Collection {
    pub title: String,
    pub docs: Option<Docs>,
    pub folders: IndexMap<Vec<String>, Option<Docs>>,
    pub variables: Vec<KeyValue>,
}
