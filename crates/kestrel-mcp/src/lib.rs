use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use kestrel_core::{Collection, Session};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt as _, tool_handler};
use serde_json::Value;
use tokio::sync::Mutex;

mod collection;
mod execution;
mod parameters;
mod tools;

#[derive(Clone)]
pub struct KestrelServer {
    root: Arc<std::path::PathBuf>,
    files: Arc<Mutex<()>>,
    session: Arc<Mutex<Session>>,
    tool_router: ToolRouter<Self>,
}

impl KestrelServer {
    pub fn new(path: &Path) -> Result<Self> {
        let collection = Collection::open(path)?;
        Ok(Self {
            root: Arc::new(collection.root.canonicalize()?),
            files: Arc::new(Mutex::new(())),
            session: Arc::new(Mutex::new(Session::new())),
            tool_router: Self::tool_router(),
        })
    }

    async fn with_collection<F>(&self, operation: F) -> Result<Value>
    where
        F: FnOnce(Collection) -> Result<Value> + Send + 'static,
    {
        let guard = self.files.clone().lock_owned().await;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            operation(Collection::open(&root)?)
        })
        .await?
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KestrelServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("kestrel-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Work with the configured Kestrel/Bruno collection. Paths are relative to its root. \
                 Read operations return saved values without interpolation. Sending requests executes \
                 collection, folder and request scripts and may change remote data. Runtime variables, \
                 cookies and OAuth tokens persist until reset_session or server exit. \
                 Keep credentials in .env and refer to them as {{process.env.NAME}}.",
            )
    }
}

fn tool_result(result: Result<Value>) -> CallToolResult {
    match result {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => {
            kestrel_core::report::report_error("execute an MCP tool", &error);
            CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))])
        }
    }
}

pub fn serve(path: &Path) -> Result<()> {
    let server = KestrelServer::new(path)?;
    kestrel_core::runtime().block_on(async move {
        server.serve(rmcp::transport::stdio()).await?.waiting().await?;
        Ok(())
    })
}
