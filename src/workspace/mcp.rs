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
use desktop_kit::mcp::Server;
use jobs::Log;
use serde_json::{json, Value};
mod tools;
use tools::*;
mod edits;
use edits::*;
mod popup;
pub(crate) use popup::*;

/// Lines of a build's log a result carries: the end is where a failure is.
const LOG_TAIL: usize = 60;

const INSTRUCTIONS: &str = "ShipScreen prepares an app for the App Store and Google Play: its identity, version \
and build numbers, store languages, screenshots and their captions, store text, and the iOS and Android builds. \
Projects are named by their folder's absolute path; list_projects gives the ones opened in ShipScreen. Show the \
user store text before saving it over theirs. Uploading to the stores is not available here: the user does that \
from the Submit step in the ShipScreen window. \
Before calling these tools, read the shipscreen skill if one is available: it has the confirmation \
rules, defaults and report layouts that these instructions leave out.";

/// The stores' limits per store text field, as the Store text step shows them.
const STORE_FIELDS: [(&str, usize); 5] = [
    ("description", 4000),
    ("whats_new", 4000),
    ("keywords", 100),
    ("promo_text", 170),
    ("short_description", 80),
];

/// What the client is told: the plumbing is desktop-kit's, the tools are below.
const SERVER: Server = Server {
    name: "shipscreen",
    version: env!("CARGO_PKG_VERSION"),
    instructions: INSTRUCTIONS,
    tools,
    library: false,
};

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
    runtime.block_on(SERVER.serve_async(|c| async move { call(&c.name, &c.args).await }));
}

// ── Tool definitions ─────────────────────────────────────────────────────

async fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_projects" => Ok(list_projects()),
        "get_project" => {
            Project::from(args).map(|p| brief(p.describe(), args["language"].as_str()))
        }
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
