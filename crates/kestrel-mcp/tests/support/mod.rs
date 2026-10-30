use std::path::PathBuf;
use std::time::Duration;

use kestrel_core::Collection;
use rmcp::ServiceExt as _;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use serde_json::Value;

pub struct Fixture {
    pub root: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!("kestrel-mcp-{}", uuid::Uuid::new_v4()));
        Collection::create(&root, "MCP test").unwrap();
        Self { root }
    }

    pub async fn connect(&self) -> RunningService<RoleClient, ()> {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_kestrel-mcp"));
        command.arg(&self.root).env("XDG_STATE_HOME", self.root.join("state")).env("NO_PROXY", "*");
        tokio::time::timeout(Duration::from_secs(15), ().serve(TokioChildProcess::new(command).unwrap())).await.unwrap().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

pub async fn call(client: &RunningService<RoleClient, ()>, name: &str, arguments: Value) -> CallToolResult {
    let parameters = CallToolRequestParams::new(name.to_string()).with_arguments(arguments.as_object().unwrap().clone());
    tokio::time::timeout(Duration::from_secs(15), client.call_tool(parameters)).await.unwrap().unwrap()
}

pub async fn value(client: &RunningService<RoleClient, ()>, name: &str, arguments: Value) -> Value {
    let result = call(client, name, arguments).await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.unwrap()
}
