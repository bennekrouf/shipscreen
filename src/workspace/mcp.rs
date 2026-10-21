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
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
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

fn tools() -> Value {
    let project =
        json!({ "type": "string", "description": "Absolute path of the project folder." });
    let codes: Vec<&str> = LOCALES.iter().map(|(code, _)| *code).collect();
    let locale = json!({ "type": "string", "enum": codes, "description": "Store language code." });
    let mut store_fields = serde_json::Map::new();
    store_fields.insert("project".into(), project.clone());
    store_fields.insert("locale".into(), locale.clone());
    for (field, limit) in STORE_FIELDS {
        store_fields.insert(
            field.into(),
            json!({ "type": "string", "description": format!("At most {limit} characters.") }),
        );
    }
    json!([
        {
            "name": "list_projects",
            "description": "The projects opened in ShipScreen, most recent first, with their app name, platform and version.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "get_project",
            "description": "Everything about one project: identity, platform, version and build numbers, languages, \
                            screens and captions per language, store text with its limits, the screenshots and \
                            builds made so far, store credentials, what each build still needs, and the release history.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
        },
        {
            "name": "set_app",
            "description": "Changes the app's identity and listing settings. Only the fields given change.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "app_name": { "type": "string", "description": "Shown under the icon, and the Play title (at most 30 characters there)." },
                    "project_slug": { "type": "string", "description": "The Dioxus project's lowercase name." },
                    "ios_bundle_id": { "type": "string" },
                    "android_package": { "type": "string" },
                    "support_url": { "type": "string", "description": "Required before App Review." },
                    "primary_color": { "type": "string", "description": "#RRGGBB, behind odd screenshots." },
                    "secondary_color": { "type": "string", "description": "#RRGGBB, behind even screenshots." },
                    "exempt_encryption": { "type": "boolean", "description": "The app only uses exempt encryption (HTTPS and the like)." },
                    "play_track": { "type": "string", "enum": ["internal", "alpha", "beta", "production"] },
                    "play_release_status": { "type": "string", "enum": ["draft", "completed"] },
                },
                "required": ["project"],
            },
        },
        {
            "name": "set_version",
            "description": "Sets the version the stores show, or bumps it, and the build numbers the next builds use.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "version": { "type": "string", "description": "e.g. 1.2.0" },
                    "bump": { "type": "string", "enum": ["major", "minor", "patch"] },
                    "ios_build_number": { "type": "integer", "minimum": 1 },
                    "android_version_code": { "type": "integer", "minimum": 1 },
                    "auto_increment_build": { "type": "boolean", "description": "Raise the build number after each successful release build." },
                },
                "required": ["project"],
            },
        },
        {
            "name": "set_languages",
            "description": "Adds or removes store languages. English (en-US) is always kept.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "add": { "type": "array", "items": locale },
                    "remove": { "type": "array", "items": locale },
                },
                "required": ["project"],
            },
        },
        {
            "name": "set_screens",
            "description": "Changes one language's raw screenshots and their captions (title and subtitle drawn \
                            above each). Screens are numbered from 1 as get_project lists them before this call; \
                            captions are applied first, then removals, then additions at the end.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "locale": locale,
                    "captions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "screen": { "type": "integer", "minimum": 1 },
                                "title": { "type": "string" },
                                "subtitle": { "type": "string" },
                            },
                            "required": ["screen"],
                        },
                    },
                    "remove": { "type": "array", "items": { "type": "integer", "minimum": 1 } },
                    "add": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Absolute path of a PNG or JPEG." },
                                "title": { "type": "string" },
                                "subtitle": { "type": "string" },
                            },
                            "required": ["path"],
                        },
                    },
                },
                "required": ["project", "locale"],
            },
        },
        {
            "name": "set_store_text",
            "description": "Sets one language's store text. Only the fields given change; one over its limit is \
                            refused. description and whats_new go to both stores (Play keeps the first 500 \
                            characters of whats_new), keywords and promo_text to the App Store, short_description \
                            to Google Play.",
            "inputSchema": { "type": "object", "properties": store_fields, "required": ["project", "locale"] },
        },
        {
            "name": "generate_screenshots",
            "description": "Draws the store-ready screenshots for every language: each screen on the primary or \
                            secondary colour under its caption, at every export size, into the project's fastlane \
                            folders. Replaces the last ones.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
        },
        {
            "name": "build",
            "description": "Runs a build, as the Build step does: ios (a signed IPA for the App Store, on a Mac with \
                            Xcode), aab (a signed bundle for Google Play) or apk (for a test device). Takes several \
                            minutes. After a release build the build number goes up when auto-increment is on.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "target": { "type": "string", "enum": ["ios", "aab", "apk"] },
                },
                "required": ["project", "target"],
            },
        },
    ])
}

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

impl Project {
    fn from(args: &Value) -> Result<Self, String> {
        let dir = PathBuf::from(text(args, "project")?);
        if !dir.is_absolute() {
            return Err("project must be an absolute path".into());
        }
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        let known = load_settings().recent_projects.iter().any(|p| p == &dir);
        if !known
            && !dir.join("shipscreen.json").is_file()
            && !dir.join("appscreens.json").is_file()
        {
            return Err(format!(
                "{} is not a ShipScreen project: open it once in ShipScreen, or pick one from list_projects",
                dir.display()
            ));
        }
        Ok(Self { dir })
    }

    /// The state as on disk now, seeded from the project's own files the way
    /// the window seeds it when it opens the project.
    fn state(&self) -> ProjectState {
        let mut state = load_project_state(&self.dir);
        let identified = seed_identity(&mut state, &self.dir);
        if seed_release_fields(&mut state, &self.dir, &load_settings()) | identified {
            save_project_state(&self.dir, &state);
        }
        state
    }

    /// Applies `f` to the state as on disk now and saves it.
    fn change<R>(
        &self,
        f: impl FnOnce(&mut ProjectState) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut state = self.state();
        let r = f(&mut state)?;
        save_project_state(&self.dir, &state);
        Ok(r)
    }

    fn describe(&self) -> Value {
        describe(&self.dir, &self.state(), &load_settings())
    }
}

fn describe(dir: &std::path::Path, p: &ProjectState, settings: &Settings) -> Value {
    let languages: Vec<Value> = p
        .locales
        .iter()
        .map(|c| json!({ "code": c, "name": locale_name(c) }))
        .collect();
    let mut screens = serde_json::Map::new();
    let mut store = serde_json::Map::new();
    for locale in &p.locales {
        let texts = p.texts_for(locale);
        let list: Vec<Value> = p
            .sources_for(locale)
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let (title, subtitle) = texts.get(i).cloned().unwrap_or_default();
                json!({ "screen": i + 1, "path": path, "exists": path.is_file(), "title": title, "subtitle": subtitle })
            })
            .collect();
        screens.insert(locale.clone(), Value::Array(list));
        let t = p.store_texts.get(locale).cloned().unwrap_or_default();
        let mut fields = serde_json::Map::new();
        for (field, limit) in STORE_FIELDS {
            let value = store_field(&t, field).to_string();
            fields.insert(
                field.into(),
                json!({ "text": value, "chars": value.chars().count(), "limit": limit }),
            );
        }
        store.insert(locale.clone(), Value::Object(fields));
    }

    let artifact = |a: &Option<Artifact>| {
        a.as_ref()
            .map(|a| json!({ "path": a.path, "made": ago(a.modified) }))
    };
    let found = scan_artifacts(dir);
    let creds = read_creds(dir);
    let inputs = jobs::BuildInputs::of(dir, p, settings);
    let mut builds = serde_json::Map::new();
    for (target, script, applies) in [
        (
            "ios",
            "build_ios_distribution.sh",
            p.platform_type.has_ios(),
        ),
        (
            "aab",
            "build_android_release.sh",
            p.platform_type.has_android(),
        ),
        ("apk", "build_apk.sh", p.platform_type.has_android()),
    ] {
        if applies {
            builds.insert(
                target.into(),
                json!(inputs.problem(script).unwrap_or_else(|| "ready".into())),
            );
        }
    }
    let checks = |list: &[CredCheck]| -> Vec<Value> {
        list.iter()
            .map(|c| json!({ "check": c.label, "ok": c.ok, "detail": c.detail }))
            .collect()
    };
    let (signing, _) = resolve_android_signing(dir, &settings.android_keystore_path, &key_alias(p));

    json!({
        "project": dir,
        "app_name": p.app_name,
        "project_slug": p.project_slug,
        "platform": p.platform_type.label(),
        "ios_bundle_id": p.ios_bundle_id,
        "android_package": p.android_bundle_id,
        "support_url": p.support_url,
        "primary_color": p.primary_color,
        "secondary_color": p.secondary_color,
        "exempt_encryption": p.exempt_encryption,
        "play_track": p.play_track,
        "play_release_status": p.play_release_status,
        "version": p.version,
        "ios_build_number": p.ios_build_number,
        "android_version_code": p.android_version_code,
        "auto_increment_build": p.auto_increment_build,
        "languages": languages,
        "screens": screens,
        "store_text": store,
        "screenshots_generated": p.output_paths.len(),
        "builds_made": { "ipa": artifact(&found.ipa), "aab": artifact(&found.aab), "apk": artifact(&found.apk) },
        "build_readiness": builds,
        "credentials": {
            "app_store": checks(&creds.app_store),
            "google_play": checks(&creds.play),
            "android_upload_key": checks(&signing.checks()),
            "apple_identity": settings.apple_identity,
            "provisioning_profile": p.provisioning_profile,
        },
        "releases": p.releases.iter().take(10).map(|r| json!({
            "at": r.at, "platform": r.platform, "version": r.version, "build": r.build, "action": r.action,
        })).collect::<Vec<_>>(),
    })
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

fn set_app(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    project.change(|p| {
        let s = |key: &str| args[key].as_str().map(|v| v.trim().to_string());
        if let Some(v) = s("app_name") {
            p.app_name = v;
        }
        if let Some(v) = s("project_slug") {
            p.project_slug = v;
        }
        if let Some(v) = s("ios_bundle_id") {
            p.ios_bundle_id = v;
        }
        if let Some(v) = s("android_package") {
            p.android_bundle_id = v;
        }
        if let Some(v) = s("support_url") {
            p.support_url = v;
        }
        for (key, slot) in [
            ("primary_color", &mut p.primary_color),
            ("secondary_color", &mut p.secondary_color),
        ] {
            if let Some(v) = s(key) {
                if parse_hex_color(&v).is_none() {
                    return Err(format!("{key} is a colour like #3B82F6, not {v}"));
                }
                *slot = v;
            }
        }
        if let Some(v) = args["exempt_encryption"].as_bool() {
            p.exempt_encryption = v;
        }
        if let Some(v) = s("play_track") {
            if !["internal", "alpha", "beta", "production"].contains(&v.as_str()) {
                return Err(format!(
                    "play_track is internal, alpha, beta or production, not {v}"
                ));
            }
            p.play_track = v;
        }
        if let Some(v) = s("play_release_status") {
            if !["draft", "completed"].contains(&v.as_str()) {
                return Err(format!(
                    "play_release_status is draft or completed, not {v}"
                ));
            }
            p.play_release_status = v;
        }
        Ok(())
    })?;
    Ok(project.describe())
}

fn set_version(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let v = project.change(|p| {
        if let Some(v) = args["version"].as_str().map(str::trim) {
            if !valid_version(v) {
                return Err(format!(
                    "{v} is not a version the stores take; use numbers like 1.2.0"
                ));
            }
            p.version = v.to_string();
        }
        if let Some(b) = args["bump"].as_str() {
            let idx = match b {
                "major" => 0,
                "minor" => 1,
                "patch" => 2,
                other => return Err(format!("bump is major, minor or patch, not {other}")),
            };
            p.version = bump_version(&p.version, idx);
        }
        let number = |key: &str| {
            args[key]
                .as_u64()
                .map(|n| u32::try_from(n).unwrap_or(u32::MAX).max(1))
        };
        if let Some(n) = number("ios_build_number") {
            p.ios_build_number = n;
        }
        if let Some(n) = number("android_version_code") {
            p.android_version_code = n;
        }
        if let Some(on) = args["auto_increment_build"].as_bool() {
            p.auto_increment_build = on;
        }
        Ok(json!({
            "version": p.version,
            "ios_build_number": p.ios_build_number,
            "android_version_code": p.android_version_code,
            "auto_increment_build": p.auto_increment_build,
        }))
    })?;
    Ok(v)
}

fn set_languages(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let list = |key: &str| -> Result<Vec<&'static str>, String> {
        args[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| known_locale(v.as_str().unwrap_or("")))
            .collect()
    };
    let (add, remove) = (list("add")?, list("remove")?);
    if remove.contains(&"en-US") {
        return Err(
            "English (en-US) is always kept: it is the default language in both stores".into(),
        );
    }
    project.change(|p| {
        p.locales.retain(|l| !remove.contains(&l.as_str()));
        for code in add {
            if !p.locales.iter().any(|l| l == code) {
                p.locales.push(code.to_string());
                p.locale_sources.entry(code.to_string()).or_default();
                p.ensure_texts_len(code, 0);
            }
        }
        Ok(())
    })?;
    Ok(project.describe())
}

fn set_screens(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let locale = known_locale(&text(args, "locale")?)?;
    project.change(|p| {
        if !p.locales.iter().any(|l| l == locale) {
            return Err(format!(
                "{locale} is not one of the project's languages: add it with set_languages"
            ));
        }
        let mut sources = p.sources_for(locale);
        let mut texts = p.texts_for(locale);
        texts.resize(
            sources.len().max(texts.len()),
            (String::new(), String::new()),
        );
        let count = sources.len();
        let screen = |n: u64| -> Result<usize, String> {
            (n as usize)
                .checked_sub(1)
                .filter(|i| *i < count)
                .ok_or(format!("no screen {n}: {locale} has {count}"))
        };
        for c in args["captions"].as_array().into_iter().flatten() {
            let i = screen(
                c["screen"]
                    .as_u64()
                    .ok_or("each caption needs its screen number")?,
            )?;
            if let Some(t) = c["title"].as_str() {
                texts[i].0 = t.trim().to_string();
            }
            if let Some(t) = c["subtitle"].as_str() {
                texts[i].1 = t.trim().to_string();
            }
        }
        let mut gone: Vec<usize> = args["remove"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| screen(v.as_u64().unwrap_or(0)))
            .collect::<Result<_, _>>()?;
        gone.sort_unstable();
        gone.dedup();
        for i in gone.into_iter().rev() {
            sources.remove(i);
            if i < texts.len() {
                texts.remove(i);
            }
        }
        texts.truncate(sources.len());
        for a in args["add"].as_array().into_iter().flatten() {
            let path = PathBuf::from(
                a["path"]
                    .as_str()
                    .ok_or("each screen added needs its path")?,
            );
            let image = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["png", "jpg", "jpeg"].contains(&e.to_ascii_lowercase().as_str()));
            if !path.is_absolute() || !path.is_file() || !image {
                return Err(format!(
                    "{} is not a PNG or JPEG file (an absolute path)",
                    path.display()
                ));
            }
            if sources.contains(&path) {
                continue;
            }
            sources.push(path);
            let caption = |key: &str| a[key].as_str().unwrap_or("").trim().to_string();
            texts.push((caption("title"), caption("subtitle")));
        }
        p.locale_sources.insert(locale.to_string(), sources);
        p.locale_texts.insert(locale.to_string(), texts);
        Ok(())
    })?;
    Ok(project.describe()["screens"][locale].clone())
}

fn set_store_text(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let locale = known_locale(&text(args, "locale")?)?;
    for (field, limit) in STORE_FIELDS {
        if let Some(v) = args[field].as_str() {
            let n = v.trim().chars().count();
            if n > limit {
                return Err(format!("{field} is {n} characters; the stores take at most {limit}. Nothing was saved."));
            }
        }
    }
    project.change(|p| {
        if !p.locales.iter().any(|l| l == locale) {
            return Err(format!(
                "{locale} is not one of the project's languages: add it with set_languages"
            ));
        }
        let t = p.store_texts.entry(locale.to_string()).or_default();
        for (field, _) in STORE_FIELDS {
            if let Some(v) = args[field].as_str() {
                *store_field_mut(t, field) = v.trim().to_string();
            }
        }
        Ok(())
    })?;
    Ok(project.describe()["store_text"][locale].clone())
}

/// A job's output, collected for the result.
fn lines() -> (Log, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
    let lines = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    (Log::Lines(lines.clone()), lines)
}

fn tail(lines: &[String]) -> Vec<String> {
    lines[lines.len().saturating_sub(LOG_TAIL)..].to_vec()
}

async fn generate(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let state = project.state();
    let (log, collected) = lines();
    let mut previews = Vec::new();
    let outputs = jobs::generate_manual_images(project.dir.clone(), state, log, |enc| {
        previews.push(format!("/localimg/{enc}"))
    })
    .await?;
    let count = outputs.len();
    project.change(|p| {
        p.generated_urls = previews;
        p.output_paths = outputs;
        Ok(())
    })?;
    let folders: std::collections::BTreeSet<String> = project
        .state()
        .output_paths
        .iter()
        .filter_map(|(_, path)| path.parent().map(|d| d.display().to_string()))
        .collect();
    let log = collected.borrow().clone();
    Ok(json!({ "files": count, "folders": folders, "log": tail(&log) }))
}

async fn run(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let script = match text(args, "target")?.as_str() {
        "ios" => "build_ios_distribution.sh",
        "aab" => "build_android_release.sh",
        "apk" => "build_apk.sh",
        other => return Err(format!("target is ios, aab or apk, not {other}")),
    };
    let inputs = jobs::BuildInputs::of(&project.dir, &project.state(), &load_settings());
    if let Some(problem) = inputs.problem(script) {
        return Err(problem);
    }
    let (log, collected) = lines();
    let outcome = jobs::build(inputs, script.to_string(), log).await;
    let log = collected.borrow().clone();
    match outcome {
        Ok(()) => {
            let numbers = project.change(|p| {
                jobs::bump_after_build(p, script);
                Ok(json!({ "next_ios_build_number": p.ios_build_number, "next_android_version_code": p.android_version_code }))
            })?;
            let found = scan_artifacts(&project.dir);
            let made = match script {
                "build_ios_distribution.sh" => found.ipa,
                "build_android_release.sh" => found.aab,
                _ => found.apk,
            };
            Ok(json!({ "built": made.map(|a| a.path), "numbers": numbers, "log": tail(&log) }))
        }
        Err(e) => Err(format!(
            "{e}\n\nLast lines of the build:\n{}",
            tail(&log).join("\n")
        )),
    }
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

#[component]
pub(crate) fn McpPopup(on_close: EventHandler<()>) -> Element {
    let command = command();
    let desktop = desktop_config(&command);
    let code = code_command(&command);
    rsx! {
        div { class: "settings-backdrop", onclick: move |_| on_close.call(()) }
        div {
            class: "settings-popup settings-popup-wide",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Use ShipScreen from Claude",
            tabindex: "-1",
            onmounted: move |e: Event<MountedData>| {
                spawn(async move { let _ = e.set_focus(true).await; });
            },
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    e.stop_propagation();
                    on_close.call(());
                }
            },
            div { class: "settings-header",
                h2 { "Use ShipScreen from Claude" }
                button {
                    class: "btn btn-icon",
                    title: "Close",
                    aria_label: "Close",
                    onclick: move |_| on_close.call(()),
                    {icon_close()}
                }
            }
            p { class: "settings-hint mcp-lead",
                "Ask Claude to fill in your store listing: write and translate the store text, caption the \
                 screenshots in every language, bump the version, generate the screenshots and run a build. It \
                 works on the same projects as this window, and what it changes shows up here within a couple of \
                 seconds. Uploading to the stores stays here, in the Submit step."
            }

            p { class: "settings-section-title", "1 · Claude Desktop" }
            p { class: "settings-hint",
                "Settings \u{2192} Developer \u{2192} Edit Config, paste this in and save, then quit Claude and open \
                 it again. If the file already has an mcpServers section, add just the shipscreen entry to it."
            }
            Snippet { text: desktop.clone() }
            p { class: "settings-hint mcp-path", "The file: {desktop_config_path()}" }

            p { class: "settings-section-title", "2 · Claude Code" }
            p { class: "settings-hint", "Run this once in a terminal:" }
            Snippet { text: code.clone() }

            p { class: "settings-section-title", "3 · Try it" }
            p { class: "settings-hint",
                "Ask: \u{201c}What does my project still need before a release?\u{201d}, or \u{201c}Translate the \
                 store text and captions into French and generate the screenshots.\u{201d}"
            }
            p { class: "settings-hint mcp-note",
                "Claude in the browser (claude.ai) can\u{2019}t use this: it only connects to servers on the internet, \
                 and ShipScreen runs on your computer. Any other app that speaks MCP can, with the same command: "
                code { "{command} mcp" }
            }
        }
    }
}

/// A block of text to paste somewhere else, with its own Copy button, as the
/// Accounts step copies values.
#[component]
fn Snippet(text: String) -> Element {
    let mut copied = use_signal(|| false);
    let to_copy = text.clone();
    rsx! {
        div { class: "mcp-snippet-wrap",
            pre { class: "mcp-snippet", "{text}" }
            button {
                class: "btn btn-sm mcp-copy",
                onclick: move |_| {
                    let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&to_copy).unwrap_or_default());
                    document::eval(&js);
                    copied.set(true);
                },
                if copied() { "Copied ✓" } else { "Copy" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_tool_has_a_unique_name_and_an_object_schema() {
        let tools = tools();
        let tools = tools.as_array().unwrap();
        let names: HashSet<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), tools.len());
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            for required in t["inputSchema"]["required"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let key = required.as_str().unwrap();
                assert!(
                    t["inputSchema"]["properties"].get(key).is_some(),
                    "{} requires {key} undescribed",
                    t["name"]
                );
            }
        }
    }

    #[test]
    fn languages_are_the_ones_shipscreen_has() {
        assert_eq!(known_locale("fr-fr").unwrap(), "fr-FR");
        assert!(known_locale("de-DE").is_err());
    }

    #[test]
    fn a_project_must_be_an_absolute_folder() {
        assert!(Project::from(&json!({ "project": "app" })).is_err());
        assert!(Project::from(&json!({ "project": "/nowhere/app" })).is_err());
    }

    #[test]
    fn the_desktop_entry_starts_this_app_in_mcp_mode() {
        let config: Value = serde_json::from_str(&desktop_config(
            "/Applications/ShipScreen.app/Contents/MacOS/shipscreen",
        ))
        .unwrap();
        assert_eq!(
            config["mcpServers"]["shipscreen"]["command"],
            "/Applications/ShipScreen.app/Contents/MacOS/shipscreen"
        );
        assert_eq!(config["mcpServers"]["shipscreen"]["args"], json!(["mcp"]));
        let path = r"C:\Program Files\ShipScreen\shipscreen.exe";
        let config: Value = serde_json::from_str(&desktop_config(path)).unwrap();
        assert_eq!(config["mcpServers"]["shipscreen"]["command"], path);
        assert_eq!(
            code_command("/A B/shipscreen"),
            "claude mcp add shipscreen --scope user -- \"/A B/shipscreen\" mcp"
        );
    }
}
