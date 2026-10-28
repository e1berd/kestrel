//! The engine of Kestrel (code name volley): collections kept as files in git, in the OpenCollection
//! YAML format Bruno writes, and everything it takes to send their requests.

pub mod collection;
pub mod format;
pub mod http;
pub mod oauth2;
pub mod script;
pub mod session;
pub mod vars;
pub mod yaml;

pub use collection::{Collection, Document, Environment, Folder, Item, Request};
pub use http::{Outcome, Plan, Response, send};
pub use session::Session;

/// The runtime requests go out on, shared by the app; the CLI brings its own.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kestrel-http")
            .enable_all()
            .build()
            .expect("the HTTP runtime")
    })
}
