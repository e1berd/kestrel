use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::parameters::{CreateRequest, Execute, RequestPath, UpdateRequest};
use crate::{KestrelServer, collection, tool_result};

#[tool_router(vis = "pub(crate)")]
impl KestrelServer {
    #[tool(description = "Get the configured collection's name, request count and environment names", annotations(read_only_hint = true))]
    async fn collection_info(&self) -> CallToolResult {
        tool_result(self.with_collection(collection::info).await)
    }

    #[tool(description = "List saved requests with relative paths, methods and uninterpolated URLs", annotations(read_only_hint = true))]
    async fn list_requests(&self) -> CallToolResult {
        tool_result(self.with_collection(collection::list).await)
    }

    #[tool(
        description = "Read a saved OpenCollection request including headers, body, auth, scripts and unknown keys",
        annotations(read_only_hint = true)
    )]
    async fn get_request(&self, Parameters(arguments): Parameters<RequestPath>) -> CallToolResult {
        tool_result(self.with_collection(move |collection| collection::get(collection, &arguments.path)).await)
    }

    #[tool(
        description = "List environments and saved variables; values marked secret are hidden and .env is never returned",
        annotations(read_only_hint = true)
    )]
    async fn list_environments(&self) -> CallToolResult {
        tool_result(self.with_collection(collection::environments).await)
    }

    #[tool(
        description = "Create an HTTP request in an existing collection folder using Bruno-compatible YAML",
        annotations(read_only_hint = false, destructive_hint = false, open_world_hint = false)
    )]
    async fn create_request(&self, Parameters(arguments): Parameters<CreateRequest>) -> CallToolResult {
        tool_result(self.with_collection(move |collection| collection::create(collection, arguments)).await)
    }

    #[tool(
        description = "Patch a saved request, preserving omitted fields, unknown keys and YAML layout. Use process.env placeholders for secrets",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = false)
    )]
    async fn update_request(&self, Parameters(arguments): Parameters<UpdateRequest>) -> CallToolResult {
        tool_result(self.with_collection(move |collection| collection::update(collection, arguments)).await)
    }

    #[tool(
        description = "Send exactly one saved request with inherited auth, variables, scripts and tests. May modify remote data. Response bodies are limited to 64 KiB",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = true)
    )]
    async fn send_request(&self, Parameters(arguments): Parameters<Execute>) -> CallToolResult {
        tool_result(self.execute(arguments, true).await)
    }

    #[tool(
        description = "Run a request, folder or entire collection in order, honoring stop/skip/setNextRequest scripts. May modify remote data. Returns responses and test results",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = true)
    )]
    async fn run_collection(&self, Parameters(arguments): Parameters<Execute>) -> CallToolResult {
        tool_result(self.execute(arguments, false).await)
    }

    #[tool(
        description = "Clear this server session's runtime variables, environment overrides, cookies and OAuth tokens",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = false)
    )]
    async fn reset_session(&self) -> CallToolResult {
        *self.session.lock().await = kestrel_core::Session::new();
        tool_result(Ok(serde_json::json!({"reset": true})))
    }
}
