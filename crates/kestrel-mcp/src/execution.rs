use std::time::Duration;

use anyhow::{Result, bail, ensure};
use futures::FutureExt as _;
use futures::future::BoxFuture;
use kestrel_core::oauth2::Interact;
use kestrel_core::{Item, Outcome, Plan};
use serde_json::{Value, json};

use crate::KestrelServer;
use crate::collection::{relative, resolve};
use crate::parameters::Execute;

struct NonInteractive;

impl Interact for NonInteractive {
    fn authorize(&self, _: String, _: String) -> BoxFuture<'static, Result<String>> {
        async { bail!("interactive OAuth sign-in is unavailable over MCP; use client credentials or a token variable") }.boxed()
    }
}

impl KestrelServer {
    pub(crate) async fn execute(&self, arguments: Execute, single: bool) -> Result<Value> {
        let maximum = arguments.max_requests.unwrap_or(100);
        let timeout = arguments.timeout_seconds.unwrap_or(60);
        ensure!((1..=1000).contains(&maximum), "max_requests must be between 1 and 1000");
        ensure!((1..=300).contains(&timeout), "timeout_seconds must be between 1 and 300");
        let session = self.session.lock().await;
        let plans = self.plans(arguments.path, arguments.environment, single).await?;
        session.lock().runtime.extend(arguments.variables);
        let mut results = Vec::new();
        let mut index = 0;
        let mut passed = true;
        let mut error = None;
        while index < plans.len() {
            if results.len() >= maximum {
                error = Some(format!("execution stopped after {maximum} requests; increase max_requests or check setNextRequest loops"));
                break;
            }
            let (path, plan) = &plans[index];
            let outcome =
                match tokio::time::timeout(Duration::from_secs(timeout), kestrel_core::send(plan, &session, &NonInteractive)).await {
                    Ok(outcome) => outcome,
                    Err(error) => Outcome { error: Some(format!("request timed out after {timeout}s: {error}")), ..Default::default() },
                };
            for error in [&outcome.error, &outcome.script_error].into_iter().flatten() {
                kestrel_core::report::report_error("execute an MCP request", error);
            }
            for test in outcome.tests.iter().filter(|test| !test.passed) {
                kestrel_core::report::report_error("run an MCP request test", test.error.as_deref().unwrap_or(&test.name));
            }
            passed &= outcome.passed();
            results.push(outcome_value(path, &outcome));
            if single || outcome.stop || (arguments.bail && !outcome.passed()) {
                break;
            }
            index = match &outcome.next_request {
                Some(name) => match plans.iter().position(|(_, plan)| &plan.name == name) {
                    Some(index) => index,
                    None => {
                        error = Some(format!("no request named `{name}` in this run"));
                        break;
                    }
                },
                None => index + 1,
            };
        }
        if let Some(error) = &error {
            kestrel_core::report::report_error("run an MCP collection", error);
        }
        Ok(json!({"passed": passed && error.is_none(), "error": error, "executed": results.len(), "results": results}))
    }

    async fn plans(&self, path: String, environment: Option<String>, single: bool) -> Result<Vec<(String, Plan)>> {
        let guard = self.files.clone().lock_owned().await;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let collection = kestrel_core::Collection::open(&root)?;
            let target = resolve(&collection, &path)?;
            if single {
                ensure!(collection.request(&target).is_some(), "path must name a saved request");
            } else {
                ensure!(
                    target == collection.root || matches!(collection.item(&target), Some(Item::Folder(_) | Item::Request(_))),
                    "path must name a request or collection folder"
                );
            }
            let plans = collection
                .requests()
                .into_iter()
                .filter(|request| request.doc.path == target || (!single && request.doc.path.starts_with(&target)))
                .map(|request| {
                    resolve(&collection, &relative(&collection, &request.doc.path)?)?;
                    Ok((relative(&collection, &request.doc.path)?, Plan::new(&collection, &request.doc.path, environment.as_deref())?))
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(!plans.is_empty(), "there are no requests at this path");
            Ok(plans)
        })
        .await?
    }
}

fn outcome_value(path: &str, outcome: &Outcome) -> Value {
    const BODY_LIMIT: usize = 64 * 1024;
    json!({
        "path": path,
        "passed": outcome.passed(),
        "skipped": outcome.skipped,
        "error": outcome.error,
        "script_error": outcome.script_error,
        "tests": outcome.tests,
        "logs": outcome.logs,
        "response": outcome.response.as_ref().map(|response| json!({
            "status": response.status,
            "status_text": response.status_text,
            "url": response.url,
            "headers": response.headers,
            "elapsed_ms": response.elapsed.as_millis() as u64,
            "body": String::from_utf8_lossy(&response.body[..response.body.len().min(BODY_LIMIT)]),
            "body_bytes": response.body.len(),
            "body_truncated": response.body.len() > BODY_LIMIT,
        })),
    })
}
