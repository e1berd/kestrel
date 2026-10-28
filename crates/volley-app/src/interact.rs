//! Signing in for OAuth2 from the app: the browser opens, and the redirect is caught on this machine
//! when the callback URL points here, or pasted into a dialog when it points elsewhere.

use anyhow::{Context as _, Result};
use futures::FutureExt as _;
use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use volley_core::oauth2::{Interact, browser_and_loopback};

/// A request to show the paste dialog; the answer goes back on `reply`.
pub struct PastePrompt {
    pub callback: String,
    pub reply: oneshot::Sender<String>,
}

#[derive(Clone)]
pub struct GuiInteract {
    pub prompts: mpsc::UnboundedSender<PastePrompt>,
}

impl Interact for GuiInteract {
    fn authorize(&self, authorization_url: String, callback_url: String) -> BoxFuture<'static, Result<String>> {
        let prompts = self.prompts.clone();
        async move {
            if let Some(url) = browser_and_loopback(&authorization_url, &callback_url).await? {
                return Ok(url);
            }
            let (reply, answer) = oneshot::channel();
            prompts.unbounded_send(PastePrompt { callback: callback_url, reply }).context("the app is closing")?;
            answer.await.context("the sign-in was cancelled")
        }
        .boxed()
    }
}
