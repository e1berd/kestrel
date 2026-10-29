pub mod collection;
pub mod document;
pub mod format;
pub mod http;
mod jwt;
mod loading;
pub mod oauth2;
pub mod outcome;
pub mod plan;
pub mod report;
mod request_builder;
pub mod script;
pub mod session;
pub mod variable_home;
pub mod vars;
pub mod yaml;

pub use collection::{Collection, Environment, Folder, Item, Request};
pub use document::Document;
pub use http::send;
pub use outcome::{Outcome, Response, SentRequest};
pub use plan::Plan;
pub use session::Session;
pub use variable_home::VariableHome;

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
