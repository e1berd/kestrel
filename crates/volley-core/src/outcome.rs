use std::time::Duration;

use serde_json::Value as Json;

use crate::script::{LogLine, TestResult};

#[derive(Debug, Clone, Default)]
pub struct SentRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}
#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub elapsed: Duration,
    pub url: String,
    pub version: String,
}
impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn content_type(&self) -> String {
        self.header("content-type").unwrap_or_default().to_lowercase()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn json(&self) -> Option<Json> {
        serde_json::from_slice(&self.body).ok()
    }

    pub fn pretty(&self) -> String {
        match self.json() {
            Some(json) if !self.body.is_empty() => serde_json::to_string_pretty(&json).unwrap_or_else(|_| self.text()),
            _ => self.text(),
        }
    }

    pub fn language(&self) -> &'static str {
        let ct = self.content_type();
        if ct.contains("json") || self.json().is_some() {
            "json"
        } else if ct.contains("html") {
            "html"
        } else if ct.contains("xml") {
            "xml"
        } else if ct.contains("javascript") {
            "javascript"
        } else {
            "text"
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub sent: Option<SentRequest>,
    pub response: Option<Response>,
    pub error: Option<String>,
    pub script_error: Option<String>,
    pub tests: Vec<TestResult>,
    pub logs: Vec<LogLine>,
    pub skipped: bool,
    pub next_request: Option<String>,
    pub stop: bool,
}
impl Outcome {
    pub(crate) fn failed(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn passed(&self) -> bool {
        self.error.is_none() && self.script_error.is_none() && self.tests.iter().all(|t| t.passed)
    }
}
