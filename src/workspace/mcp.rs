//! `shipscreen mcp` — ShipScreen's projects, served to an MCP client over
//! stdio, and the window that says how to connect one.
//!
//! No window, no Dioxus: the same project state, screenshot generation and
//! builds the workspace uses, driven from tool calls instead. A project is
//! named by its folder; its state is its `shipscreen.json`, which the window
//! may have open too. So every change here reads the file, applies the change
//! and writes it back at once, and the window loads what changed (see the
//! watcher in `ProjectView`).
//!
//! Store uploads (listing, IPA, App Review, Play release) are not here: they
//! publish to Apple and Google, and stay in the Submit step, where the person
//! sees the readiness checklist before each one.
//!
//! Requests are handled one at a time, so a build holds the next call until it
//! is done. Fine for one client; the place to change if that stops being true
//! is `serve`.

use super::*;
use jobs::Log;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
mod tools;
use tools::*;
mod edits;
use edits::*;
mod popup;
pub(crate) use popup::*;

/// The newest protocol revision this server speaks. A client asking for an
/// older one gets its own back; everything used here exists in all of them.
const PROTOCOL: &str = "2025-06-18";

/// Lines of a build's log a result carries: the end is where a failure is.
const LOG_TAIL: usize = 60;

const INSTRUCTIONS: &str = "ShipScreen prepares an app for the App Store and Google Play: its identity, version \
and build numbers, store languages, the raw screenshots and their captions per language, the store text, the \
store-ready screenshots, and the iOS and Android builds. Projects are named by their folder's absolute path; \
list_projects gives the ones opened in ShipScreen. get_project shows everything about one, including what is still \
missing for a build. Store text has the stores' character limits, which are enforced; Claude is good at writing and \
translating it, but show the user the text before saving it over theirs. generate_screenshots draws the \
store-ready images from the screens and captions (the simple style; the AI style is in the ShipScreen window). \
build runs a release build and takes minutes. Uploading to the stores is not available here: the user does that \
from the Submit step in the ShipScreen window.";

/// The stores' limits per store text field, as the Store text step shows them.
const STORE_FIELDS: [(&str, usize); 5] = [
    ("description", 4000),
    ("whats_new", 4000),
    ("keywords", 100),
    ("promo_text", 170),
    ("short_description", 80),
];

pub(crate) fn serve() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("shipscreen mcp: could not start: {e}");
            std::process::exit(1);
        }
    };
    runtime.block_on(async {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                write(&mut stdout, &error(Value::Null, -32700, "not JSON"));
                continue;
            };
            // A notification (no id) asks for nothing back: `initialized`,
            // cancellations, and whatever later revisions add.
            let Some(id) = message.get("id").cloned() else {
                continue;
            };
            let reply = handle(id, &message).await;
            write(&mut stdout, &reply);
        }
    });
}

async fn handle(id: Value, message: &Value) -> Value {
    let method = message["method"].as_str().unwrap_or("");
    let params = &message["params"];
    match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOL);
            let version = if asked <= PROTOCOL { asked } else { PROTOCOL };
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "shipscreen", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let (text, is_error) = match call(name, &params["arguments"]).await {
                Ok(value) => (pretty(&value), false),
                Err(why) => (why, true),
            };
            result(
                id,
                json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
            )
        }
        _ => error(id, -32601, &format!("no method {method}")),
    }
}

fn write(stdout: &mut std::io::Stdout, reply: &Value) {
    // One message per line, and nothing else ever on stdout: the client reads
    // it as the protocol, so a stray print is a corrupt message.
    let mut bytes = serde_json::to_vec(reply).unwrap_or_default();
    bytes.push(b'\n');
    let _ = stdout.write_all(&bytes);
    let _ = stdout.flush();
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

// ── Tool definitions ─────────────────────────────────────────────────────

async fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_projects" => Ok(list_projects()),
        "get_project" => Project::from(args).map(|p| p.describe()),
        "set_app" => set_app(args),
        "set_version" => set_version(args),
        "set_languages" => set_languages(args),
        "set_screens" => set_screens(args),
        "set_store_text" => set_store_text(args),
        "generate_screenshots" => generate(args).await,
        "build" => run(args).await,
        _ => Err(format!("no tool {name}")),
    }
}

// ── Arguments ────────────────────────────────────────────────────────────

fn text(args: &Value, key: &str) -> Result<String, String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or(format!("{key} is required"))
}

fn known_locale(code: &str) -> Result<&'static str, String> {
    LOCALES
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code.trim()))
        .map(|(c, _)| *c)
        .ok_or_else(|| {
            let all: Vec<String> = LOCALES.iter().map(|(c, n)| format!("{c} ({n})")).collect();
            format!("no language {code}; ShipScreen has {}", all.join(", "))
        })
}

fn locale_name(code: &str) -> &'static str {
    LOCALES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or("")
}

// ── One project ──────────────────────────────────────────────────────────

/// A project named by a tool call.
struct Project {
    dir: PathBuf,
}

fn store_field<'a>(t: &'a StoreText, field: &str) -> &'a str {
    match field {
        "description" => &t.description,
        "whats_new" => &t.whats_new,
        "keywords" => &t.keywords,
        "promo_text" => &t.promo_text,
        _ => &t.short_description,
    }
}

fn store_field_mut<'a>(t: &'a mut StoreText, field: &str) -> &'a mut String {
    match field {
        "description" => &mut t.description,
        "whats_new" => &mut t.whats_new,
        "keywords" => &mut t.keywords,
        "promo_text" => &mut t.promo_text,
        _ => &mut t.short_description,
    }
}

// ── Tools ────────────────────────────────────────────────────────────────

fn list_projects() -> Value {
    let projects: Vec<Value> = load_settings()
        .recent_projects
        .iter()
        .map(|dir| {
            if !dir.is_dir() {
                return json!({ "project": dir, "missing": true });
            }
            let p = load_project_state(dir);
            json!({ "project": dir, "app_name": p.app_name, "platform": p.platform_type.label(), "version": p.version })
        })
        .collect();
    json!({ "projects": projects })
}

/// A job's output, collected for the result.
fn lines() -> (Log, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
    let lines = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    (Log::Lines(lines.clone()), lines)
}

fn tail(lines: &[String]) -> Vec<String> {
    lines[lines.len().saturating_sub(LOG_TAIL)..].to_vec()
}

// ── The window ───────────────────────────────────────────────────────────

/// This executable, as a client should start it.
fn command() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "shipscreen".into())
}

/// The `mcpServers` entry for Claude Desktop's config file. Built with serde
/// so a Windows path's backslashes arrive escaped.
pub(crate) fn desktop_config(command: &str) -> String {
    let config = json!({ "mcpServers": { "shipscreen": { "command": command, "args": ["mcp"] } } });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// The one line that adds it to Claude Code, for every project.
pub(crate) fn code_command(command: &str) -> String {
    format!("claude mcp add shipscreen --scope user -- \"{command}\" mcp")
}

/// The one line that adds it to Codex. Its CLI, IDE extension and app share
/// one list of servers, each one there for every project.
pub(crate) fn codex_command(command: &str) -> String {
    format!("codex mcp add shipscreen -- \"{command}\" mcp")
}

/// Where Claude Desktop keeps its config.
fn desktop_config_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "%APPDATA%\\Claude\\claude_desktop_config.json"
    } else if cfg!(target_os = "macos") {
        "~/Library/Application Support/Claude/claude_desktop_config.json"
    } else {
        "~/.config/Claude/claude_desktop_config.json"
    }
}

#[cfg(test)]
mod tests;
