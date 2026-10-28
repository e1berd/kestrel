use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures::future::BoxFuture;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::format::AuthConfig;
use crate::report::ReportExt as _;
use crate::session::Session;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl Token {
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|at| at - 10 <= chrono::Utc::now().timestamp())
    }

    pub fn fields(&self) -> serde_json::Map<String, Json> {
        match serde_json::to_value(self) {
            Ok(Json::Object(map)) => map,
            _ => Default::default(),
        }
    }
}

pub trait Interact: Send + Sync {
    fn authorize(&self, authorization_url: String, callback_url: String) -> BoxFuture<'static, Result<String>>;
}

pub async fn browser_and_loopback(authorization_url: &str, callback_url: &str) -> Result<Option<String>> {
    let callback = Url::parse(callback_url).context("the callback URL is not a URL")?;
    let host = callback.host_str().unwrap_or_default().to_string();
    let loopback = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1");
    let listener = if loopback {
        let port = callback.port_or_known_default().unwrap_or(80);
        let addr = if host == "localhost" { format!("127.0.0.1:{port}") } else { format!("{host}:{port}") };
        Some(tokio::net::TcpListener::bind(&addr).await.with_context(|| format!("listen for the sign-in on {addr}"))?)
    } else {
        None
    };
    open::that_detached(authorization_url).context("open the browser")?;
    let Some(listener) = listener else {
        return Ok(None);
    };
    let wait = async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await?;
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
            if !target.starts_with(callback.path()) {
                stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await.report_err("answer a stray request on the sign-in callback");
                continue;
            }
            let page = "<!doctype html><meta charset=utf-8><title>Kestrel</title><body style=\"font:16px system-ui;padding:3em\">Signed in. You can close this tab and go back to Kestrel.";
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            stream.write_all(reply.as_bytes()).await.report_err("show the signed-in page");
            let mut url = callback.clone();
            let full = Url::parse(&format!("http://local{target}"))?;
            url.set_query(full.query());
            return anyhow::Ok(url.to_string());
        }
    };
    let url = tokio::time::timeout(Duration::from_secs(300), wait).await.context("no sign-in within five minutes")??;
    Ok(Some(url))
}

pub fn token_id(config: &AuthConfig) -> String {
    let id = config.str("tokenConfig.id");
    if id.is_empty() { "credentials".into() } else { id }
}

pub async fn token(
    config: &AuthConfig,
    session: &Session,
    client: &reqwest::Client,
    interact: &dyn Interact,
    force: bool,
) -> Result<Token> {
    let id = token_id(config);
    let cached = session.lock().tokens.get(&id).cloned();
    let auto_fetch = !config.has("settings") || config.bool("settings.autoFetchToken");
    let auto_refresh = !config.has("settings") || config.bool("settings.autoRefreshToken");
    if let Some(token) = cached.filter(|_| !force) {
        if !token.is_expired() {
            return Ok(token);
        }
        if let (true, Some(refresh)) = (auto_refresh, token.refresh_token.clone()) {
            if let Ok(token) = refresh_token(config, client, &refresh).await {
                session.lock().tokens.insert(id, token.clone());
                return Ok(token);
            }
        }
    }
    if !auto_fetch && !force {
        bail!("no OAuth2 token yet for `{id}`: get one in the request's Auth tab");
    }
    let token = fetch(config, client, interact).await?;
    session.lock().tokens.insert(id, token.clone());
    Ok(token)
}

async fn fetch(config: &AuthConfig, client: &reqwest::Client, interact: &dyn Interact) -> Result<Token> {
    let flow = config.str("flow");
    let mut form: Vec<(String, String)> = Vec::new();
    match flow.as_str() {
        "client_credentials" => form.push(("grant_type".into(), "client_credentials".into())),
        "password" => {
            form.push(("grant_type".into(), "password".into()));
            form.push(("username".into(), first(config, &["resourceOwner.username", "credentials.username", "username"])));
            form.push(("password".into(), first(config, &["resourceOwner.password", "credentials.password", "password"])));
        }
        "authorization_code" | "" => {
            let callback = config.str("callbackUrl");
            let state: String = random_string(24);
            let pkce = config.has("pkce") && config.str("pkce.enabled") != "false";
            let verifier = random_string(64);
            let mut url = Url::parse(&config.str("authorizationUrl")).context("the authorization URL is not a URL")?;
            {
                let mut q = url.query_pairs_mut();
                q.append_pair("response_type", "code");
                q.append_pair("client_id", &config.str("credentials.clientId"));
                q.append_pair("redirect_uri", &callback);
                q.append_pair("state", &state);
                let scope = config.str("scope");
                if !scope.is_empty() {
                    q.append_pair("scope", &scope);
                }
                if pkce {
                    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                    q.append_pair("code_challenge", &challenge);
                    q.append_pair("code_challenge_method", "S256");
                }
            }
            let redirected = interact.authorize(url.to_string(), callback.clone()).await?;
            let redirected = Url::parse(redirected.trim()).context("that is not the URL the sign-in ended on")?;
            let get = |key: &str| redirected.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.to_string());
            if let Some(error) = get("error") {
                bail!("the sign-in failed: {error} {}", get("error_description").unwrap_or_default());
            }
            if get("state").is_some_and(|s| s != state) {
                bail!("the sign-in came back for another request (state does not match)");
            }
            let code = get("code").context("the URL has no `code` in it")?;
            form.push(("grant_type".into(), "authorization_code".into()));
            form.push(("code".into(), code));
            form.push(("redirect_uri".into(), callback));
            if pkce {
                form.push(("code_verifier".into(), verifier));
            }
        }
        other => bail!("OAuth2 flow `{other}` is not supported yet"),
    }
    let scope = config.str("scope");
    if !scope.is_empty() && flow != "authorization_code" {
        form.push(("scope".into(), scope));
    }
    exchange(config, client, &config.str("accessTokenUrl"), form).await
}

async fn refresh_token(config: &AuthConfig, client: &reqwest::Client, refresh: &str) -> Result<Token> {
    let url = config.str("refreshTokenUrl");
    let url = if url.is_empty() { config.str("accessTokenUrl") } else { url };
    let form = vec![("grant_type".into(), "refresh_token".into()), ("refresh_token".into(), refresh.to_string())];
    let mut token = exchange(config, client, &url, form).await?;
    if token.refresh_token.is_none() {
        token.refresh_token = Some(refresh.to_string());
    }
    Ok(token)
}

async fn exchange(config: &AuthConfig, client: &reqwest::Client, url: &str, mut form: Vec<(String, String)>) -> Result<Token> {
    let client_id = config.str("credentials.clientId");
    let client_secret = config.str("credentials.clientSecret");
    let in_header = config.str("credentials.placement") == "basic_auth_header";
    let mut request = client.post(url).header("Accept", "application/json");
    if in_header {
        request = request.basic_auth(&client_id, Some(&client_secret));
    } else {
        form.push(("client_id".into(), client_id));
        if !client_secret.is_empty() {
            form.push(("client_secret".into(), client_secret));
        }
    }
    let response = request.form(&form).send().await.with_context(|| format!("reach {url}"))?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        bail!("the token endpoint answered {status}: {text}");
    }
    let mut token: Token = serde_json::from_str(&text).with_context(|| format!("the token endpoint answered something else: {text}"))?;
    token.expires_at = token.expires_in.map(|s| chrono::Utc::now().timestamp() + s);
    Ok(token)
}

pub enum Placement {
    Header(String, String),
    Query(String, String),
}

pub fn placement(config: &AuthConfig, token: &Token) -> Placement {
    let value = match config.str("tokenConfig.source").as_str() {
        "id_token" => token.id_token.clone().unwrap_or_default(),
        _ => token.access_token.clone(),
    };
    let query = config.str("tokenConfig.placement.query");
    if !query.is_empty() {
        return Placement::Query(query, value);
    }
    let prefix = config.str("tokenConfig.placement.header");
    let prefix = if prefix.is_empty() && !config.has("tokenConfig") { "Bearer".to_string() } else { prefix };
    let value = if prefix.is_empty() { value } else { format!("{prefix} {value}") };
    Placement::Header("Authorization".into(), value)
}

fn first(config: &AuthConfig, paths: &[&str]) -> String {
    paths.iter().map(|p| config.str(p)).find(|v| !v.is_empty()).unwrap_or_default()
}

fn random_string(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::rng();
    (0..len).map(|_| CHARS[rng.random_range(0..CHARS.len())] as char).collect()
}
