use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, Result, bail};
use clap::Parser;
use futures::FutureExt as _;
use futures::future::BoxFuture;
use kestrel_core::oauth2::{Interact, browser_and_loopback};
use kestrel_core::{Collection, Item, Plan, Session};
use kestrel_openapi::{GroupBy, SyncOptions};

mod args;
mod codegen;
mod importing;
mod transfer;

use args::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Codegen { collection, out, language: _, single_file, archive, overwrite } => {
            codegen::generate(&collection, &out, kestrel_codegen::OutputOptions { single_file, archive }, overwrite).map(|_| true)
        }
        Command::Export { collection, out, format, encoding, overwrite } => {
            importing::export(&collection, &out, &format, &encoding, overwrite).map(|_| true)
        }
        Command::Run { path, env, vars, bail, verbose, json } => run(&path, env.as_deref(), vars, bail, verbose, json),
        Command::Import { spec, out, name, group_by, format } => {
            importing::import(&spec, &out, name.as_deref(), &group_by, &format).map(|_| true)
        }
        Command::Sync { collection, spec, group_by, prune, dry_run } => sync(&collection, spec, group_by, prune, dry_run).map(|_| true),
        Command::Envs { collection } => envs(&collection).map(|_| true),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            kestrel_core::report::report_error("run the CLI command", e);
            ExitCode::from(2)
        }
    }
}

const RED: &str = "31";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const DIM: &str = "2";
const BOLD: &str = "1";

fn paint(text: &str, code: &str) -> String {
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

struct Terminal;

impl Interact for Terminal {
    fn authorize(&self, authorization_url: String, callback_url: String) -> BoxFuture<'static, Result<String>> {
        async move {
            eprintln!("Signing in: {authorization_url}");
            if let Some(url) = browser_and_loopback(&authorization_url, &callback_url).await? {
                return Ok(url);
            }
            eprint!("Sign in in the browser, then paste the address it ended on ({callback_url}…): ");
            std::io::stderr().flush()?;
            let line = tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line).map(|_| line)
            })
            .await??;
            Ok(line.trim().to_string())
        }
        .boxed()
    }
}

fn run(path: &Path, env: Option<&str>, vars: Vec<(String, String)>, bail_early: bool, verbose: bool, json: bool) -> Result<bool> {
    let collection = Collection::open(path)?;
    let target = path.canonicalize().with_context(|| format!("{} does not exist", path.display()))?;
    let requests: Vec<PathBuf> = if target == collection.root {
        collection.requests().iter().map(|r| r.doc.path.clone()).collect()
    } else {
        match collection.item(&target) {
            Some(Item::Request(request)) => vec![request.doc.path.clone()],
            Some(Item::Folder(folder)) => {
                let mut out = Vec::new();
                collect(&folder.children, &mut out);
                out
            }
            None => bail!("{} is not a request or folder of {}", path.display(), collection.name()),
        }
    };
    if requests.is_empty() {
        bail!("there are no requests in {}", path.display());
    }
    let session = Session::new();
    {
        let mut state = session.lock();
        for (k, v) in vars {
            state.runtime.insert(k, serde_json::Value::String(v));
        }
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let mut failed = 0;
    let mut passed_tests = 0;
    let mut failed_tests = 0;
    let started = std::time::Instant::now();
    if !json {
        let env_label = env.map(|e| format!(" with {e}")).unwrap_or_default();
        println!("{} {}{}\n", paint("Kestrel", BOLD), collection.name(), paint(&env_label, DIM));
    }
    let mut index = 0;
    while index < requests.len() {
        let request_path = &requests[index];
        index += 1;
        let plan = Plan::new(&collection, request_path, env)?;
        let outcome = runtime.block_on(kestrel_core::send(&plan, &session, &Terminal));
        let relative = request_path.strip_prefix(&collection.root).unwrap_or(request_path).display().to_string();
        passed_tests += outcome.tests.iter().filter(|t| t.passed).count();
        failed_tests += outcome.tests.iter().filter(|t| !t.passed).count();
        if !outcome.passed() {
            failed += 1;
        }
        if json {
            let line = serde_json::json!({
                "request": relative,
                "method": outcome.sent.as_ref().map(|s| s.method.clone()),
                "url": outcome.sent.as_ref().map(|s| s.url.clone()),
                "status": outcome.response.as_ref().map(|r| r.status),
                "ms": outcome.response.as_ref().map(|r| r.elapsed.as_millis() as u64),
                "error": outcome.error,
                "scriptError": outcome.script_error,
                "skipped": outcome.skipped,
                "tests": outcome.tests,
                "passed": outcome.passed(),
            });
            println!("{line}");
        } else {
            print_outcome(&plan, &relative, &outcome, verbose);
        }
        if outcome.stop || (bail_early && !outcome.passed()) {
            break;
        }
        if let Some(next) = &outcome.next_request {
            match requests.iter().position(|p| collection.request(p).is_some_and(|r| &r.name() == next)) {
                Some(i) => index = i,
                None => eprintln!("{} no request named `{next}` to go on with", paint("warning:", YELLOW)),
            }
        }
    }
    if !json {
        let summary = format!(
            "{} requests, {} failed; {} tests passed, {} failed; {:.1}s",
            index,
            failed,
            passed_tests,
            failed_tests,
            started.elapsed().as_secs_f64()
        );
        println!("\n{}", if failed == 0 { paint(&summary, GREEN) } else { paint(&summary, RED) });
    }
    Ok(failed == 0)
}

fn collect(items: &[Item], out: &mut Vec<PathBuf>) {
    for item in items {
        match item {
            Item::Folder(folder) => collect(&folder.children, out),
            Item::Request(request) => out.push(request.doc.path.clone()),
        }
    }
}

fn print_outcome(plan: &Plan, relative: &str, outcome: &kestrel_core::Outcome, verbose: bool) {
    let method = outcome.sent.as_ref().map(|s| s.method.clone()).unwrap_or_else(|| plan.file.http().method.to_uppercase());
    let status = match (&outcome.response, outcome.skipped) {
        (_, true) => paint("skipped", DIM),
        (Some(r), _) => {
            let text = format!("{} {}", r.status, r.status_text);
            let colored = if r.status < 300 {
                paint(&text, GREEN)
            } else if r.status < 400 {
                paint(&text, YELLOW)
            } else {
                paint(&text, RED)
            };
            format!("{colored} {}", paint(&format!("{} ms, {}", r.elapsed.as_millis(), size(r.body.len())), DIM))
        }
        (None, _) => paint("failed", RED),
    };
    println!("{} {}  {}", paint(&format!("{method:<7}"), BOLD), relative, status);
    if let Some(sent) = &outcome.sent {
        println!("        {}", paint(&sent.url, DIM));
    }
    if let Some(error) = &outcome.error {
        println!("        {}", paint(error, RED));
    }
    if let Some(error) = &outcome.script_error {
        println!("        {}", paint(&format!("script: {error}"), RED));
    }
    for line in &outcome.logs {
        for text in line.text.lines() {
            println!("        {} {}", paint("│", DIM), text);
        }
    }
    for test in &outcome.tests {
        if test.passed {
            println!("        {} {}", paint("✓", GREEN), test.name);
        } else {
            println!("        {} {} — {}", paint("✗", RED), test.name, test.error.clone().unwrap_or_default());
        }
    }
    if verbose && let Some(response) = &outcome.response {
        for (k, v) in &response.headers {
            println!("        {}", paint(&format!("{k}: {v}"), DIM));
        }
        for line in response.pretty().lines().take(200) {
            println!("        {line}");
        }
    }
}

fn size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    }
}

fn sync(path: &Path, spec: Option<String>, group_by: Option<String>, prune: bool, dry_run: bool) -> Result<()> {
    let mut collection = Collection::open(path)?;
    let sources = collection.doc.data.openapi_sources();
    let (source, remembered_group) = match spec {
        Some(spec) => (spec, sources.first().map(|s| s.1.clone()).unwrap_or_else(|| "tags".into())),
        None => sources.first().cloned().context("the collection does not say where its OpenAPI spec is; pass --spec")?,
    };
    let group_by = GroupBy::parse(group_by.as_deref().unwrap_or(&remembered_group));
    let spec = kestrel_openapi::load(&source, &collection.root)?;
    let report = kestrel_openapi::sync(&mut collection, &spec, &SyncOptions { group_by, prune, dry_run })?;
    let verb = if dry_run { "Would sync" } else { "Synced" };
    println!("{} {} from {source}: {report}", paint(verb, GREEN), collection.name());
    for line in &report.added {
        println!("  {} {line}", paint("+", GREEN));
    }
    for line in &report.updated {
        println!("  {} {line}", paint("~", YELLOW));
    }
    for line in &report.removed {
        println!("  {} {line}", paint("-", RED));
    }
    for line in &report.stale {
        println!("  {} {line} {}", paint("?", DIM), paint("(not in the spec; --prune deletes it)", DIM));
    }
    Ok(())
}

fn envs(path: &Path) -> Result<()> {
    let collection = Collection::open(path)?;
    if collection.environments.is_empty() {
        println!("{} has no environments", collection.name());
    }
    for env in &collection.environments {
        println!("{}", paint(&env.name(), BOLD));
        for v in &env.doc.data.variables {
            let value = if v.is_secret() { "••••••".to_string() } else { v.value.clone() };
            let line = format!("  {} = {}", v.name, value);
            println!("{}", if v.enabled() { line } else { paint(&line, DIM) });
        }
    }
    Ok(())
}
