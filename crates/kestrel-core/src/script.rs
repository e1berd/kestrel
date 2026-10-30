use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use base64::Engine as _;
use indexmap::IndexMap;
use rquickjs::{Context, Ctx, Function, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::vars::Vars;

const PRELUDE: [&str; 3] = [include_str!("prelude/runtime.js"), include_str!("prelude/expect.js"), include_str!("prelude/require.js")];
const TIME_LIMIT: Duration = Duration::from_secs(30);
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReqState {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: String,
    pub body_kind: String,
    pub auth_mode: String,
    pub timeout: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResState {
    pub status: u16,
    pub status_text: String,
    pub headers: IndexMap<String, String>,
    pub body: Json,
    pub response_time: u64,
    pub url: String,
    pub size: usize,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptState {
    pub env_name: String,
    pub env_vars: Vars,
    pub runtime_vars: IndexMap<String, Json>,
    pub collection_vars: Vars,
    pub folder_vars: Vars,
    pub request_vars: Vars,
    pub process_env: Vars,
    pub oauth2: IndexMap<String, serde_json::Map<String, Json>>,
    pub request_name: String,
    pub collection_name: String,
    pub cwd: String,
    pub req: ReqState,
    pub res: Option<ResState>,
    #[serde(default)]
    pub env_changed: Vars,
    #[serde(default)]
    pub deleted_vars: Vec<String>,
    #[serde(default)]
    pub skip: bool,
    #[serde(default)]
    pub stop: bool,
    #[serde(default)]
    pub next_request: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogLine {
    pub level: String,
    pub text: String,
}
#[derive(Debug, Clone)]
pub struct ScriptRun {
    pub state: ScriptState,
    pub tests: Vec<TestResult>,
    pub logs: Vec<LogLine>,
    pub error: Option<String>,
}
#[derive(Deserialize)]
struct Finished {
    state: ScriptState,
    tests: Vec<TestResult>,
    logs: Vec<LogLine>,
}
pub fn run(scripts: &[(String, String)], state: ScriptState) -> ScriptRun {
    match run_inner(scripts, &state) {
        Ok(run) => run,
        Err(error) => ScriptRun { state, tests: Vec::new(), logs: Vec::new(), error: Some(error.to_string()) },
    }
}
fn run_inner(scripts: &[(String, String)], state: &ScriptState) -> Result<ScriptRun> {
    let runtime = Runtime::new()?;
    let started = Instant::now();
    runtime.set_interrupt_handler(Some(Box::new(move || started.elapsed() > TIME_LIMIT)));
    let context = Context::full(&runtime)?;
    let state_json = serde_json::to_string(state)?;

    let mut error: Option<String> = None;
    let finished: String = context.with(|ctx| -> Result<String> {
        install_natives(&ctx)?;
        ctx.globals().set("__state_json", state_json)?;
        eval(&ctx, &PRELUDE.concat()).map_err(|e| anyhow!("the script prelude failed: {e}"))?;
        for (label, code) in scripts {
            if code.trim().is_empty() {
                continue;
            }
            let wrapped = format!(
                "(async function () {{\n{code}\n}})().catch((e) => {{ globalThis.__failed = __message(e); }});\nPromise.all(__pending).catch(() => {{}});"
            );
            if let Err(e) = eval(&ctx, &wrapped) {
                error = Some(format!("{label}: {e}"));
                break;
            }
            while ctx.execute_pending_job() {}
            let failed: Option<String> = ctx.globals().get("__failed")?;
            if let Some(message) = failed {
                error = Some(format!("{label}: {message}"));
                break;
            }
        }
        Ok(ctx.eval::<String, _>("JSON.stringify({ state: __S, tests: __tests, logs: __logs })")?)
    })?;
    let finished: Finished = serde_json::from_str(&finished)?;
    Ok(ScriptRun { state: finished.state, tests: finished.tests, logs: finished.logs, error })
}
fn eval(ctx: &Ctx<'_>, code: &str) -> std::result::Result<(), String> {
    match ctx.eval::<(), _>(code) {
        Ok(()) => Ok(()),
        Err(rquickjs::Error::Exception) => Err(exception_text(ctx)),
        Err(e) => Err(e.to_string()),
    }
}
fn exception_text(ctx: &Ctx<'_>) -> String {
    let caught = ctx.catch();
    if let Some(exception) = caught.as_exception() {
        let message = exception.message().unwrap_or_default();
        return if message.is_empty() { "the script threw".into() } else { message };
    }
    caught.get::<rquickjs::Coerced<String>>().map(|s| s.0).unwrap_or_else(|_| "the script threw".into())
}
fn install_natives(ctx: &Ctx<'_>) -> Result<()> {
    let globals = ctx.globals();
    globals.set(
        "__native_jwt_sign",
        Function::new(ctx.clone(), |header: String, claims: String, key: String| crate::jwt::sign_for_script(&header, &claims, &key))?,
    )?;
    globals.set(
        "__native_b64decode",
        Function::new(ctx.clone(), |text: String, url_safe: bool| {
            let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = if url_safe {
                base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(cleaned.trim_end_matches('='))
            } else {
                base64::engine::general_purpose::STANDARD.decode(&cleaned)
            };
            bytes.map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default()
        })?,
    )?;
    globals.set(
        "__native_b64encode",
        Function::new(ctx.clone(), |text: String| base64::engine::general_purpose::STANDARD.encode(text.as_bytes()))?,
    )?;
    globals.set("__native_uuid", Function::new(ctx.clone(), || uuid::Uuid::new_v4().to_string())?)?;
    globals.set(
        "__native_sleep",
        Function::new(ctx.clone(), |ms: f64| std::thread::sleep(Duration::from_millis(ms.clamp(0.0, 60_000.0) as u64)))?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ScriptState {
        let mut state = ScriptState::default();
        state.env_vars.insert("sub".into(), "user-1".into());
        state.req = ReqState {
            method: "GET".into(),
            url: "{{baseUrl}}/me".into(),
            auth_mode: "bearer".into(),
            body_kind: "json".into(),
            body: "{\"a\":1}".into(),
            ..Default::default()
        };
        state
    }

    #[test]
    fn scripts_set_variables_and_headers() {
        let code = r#"
            const sub = bru.getEnvVar('sub');
            bru.setVar('who', `dev:${sub}`);
            bru.setVar('n', 3);
            req.setHeader('X-Test', 'yes');
            const body = req.getBody();
            body.b = 2;
            req.setBody(body);
            console.log({ sub });
        "#;
        let run = run(&[("before".into(), code.into())], state());
        assert_eq!(run.error, None);
        assert_eq!(run.state.runtime_vars["who"], Json::String("dev:user-1".into()));
        assert_eq!(run.state.runtime_vars["n"], serde_json::json!(3));
        assert_eq!(run.state.req.headers[0].name, "X-Test");
        assert!(run.state.req.body.contains("\"b\": 2"));
        assert!(run.logs[0].text.contains("user-1"));
    }

    #[test]
    fn tests_pass_and_fail() {
        let mut state = state();
        state.res = Some(ResState { status: 201, body: serde_json::json!({"state": "requested", "items": [1, 2]}), ..Default::default() });
        let code = r#"
            test('status', () => expect(res.status).to.be.oneOf([200, 201]));
            test('state', () => expect(res.body.state).to.equal('requested'));
            test('items', () => expect(res.body).to.have.property('items').with.lengthOf(2));
            test('wrong', () => expect(res.status).to.equal(200));
            test('deep', () => expect(res.body.items).to.eql([1, 2]));
        "#;
        let run = run(&[("tests".into(), code.into())], state);
        let passed: Vec<bool> = run.tests.iter().map(|t| t.passed).collect();
        assert_eq!(passed, vec![true, true, true, false, true]);
        assert_eq!(run.tests[3].error.as_deref(), Some("expected 201 to equal 200"));
    }

    #[test]
    fn a_throw_stops_and_says_why() {
        let run = run(&[("before-request".into(), "throw new Error('No environment selected')".into())], state());
        assert_eq!(run.error.as_deref(), Some("before-request: No environment selected"));
    }

    #[test]
    fn jsonwebtoken_signs_and_decodes() {
        let code = r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ azp: 'api' }, 'secret', { algorithm: 'HS256', issuer: 'dev', subject: 'u1', audience: 'api', expiresIn: '8h', keyid: 'k1' });
            const claims = jwt.decode(token);
            bru.setVar('ttl', claims.exp - claims.iat);
            bru.setVar('sub', claims.sub);
            bru.setVar('kid', jwt.decode(token, { complete: true }).header.kid);
        "#;
        let run = run(&[("before".into(), code.into())], state());
        assert_eq!(run.error, None);
        assert_eq!(run.state.runtime_vars["ttl"], serde_json::json!(28800));
        assert_eq!(run.state.runtime_vars["sub"], Json::String("u1".into()));
        assert_eq!(run.state.runtime_vars["kid"], Json::String("k1".into()));
    }
}
