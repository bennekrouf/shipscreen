//! Project workspace: a sidebar of steps in the order the work actually
//! happens — Setup → Build → Store listing → Release — with one screen per
//! step and a job drawer at the bottom for anything long-running.
//!
//! State shared by every step lives in [`Ws`], provided as context by
//! [`ProjectView`]. Step statuses are computed from that state (and from files
//! on disk), never stored, so they can't drift from reality.

use super::*;
use std::time::SystemTime;

mod accounts;
mod android_build;
mod asckey;
mod checks;
mod consistency;
mod doctor;
mod envfile;
mod java;
mod jobs;
#[cfg(not(target_os = "android"))]
pub(crate) mod mcp;
#[cfg(target_os = "android")]
mod mcp {
    use dioxus::prelude::*;
    /// Nothing to connect to on Android: Claude Desktop and Claude Code run on computers.
    #[component]
    pub(crate) fn McpPopup(on_close: EventHandler<()>) -> Element {
        let _ = on_close;
        rsx! {}
    }
}
mod playkey;
mod repo;
mod shellenv;
mod signing;
mod steps;
mod stores;
mod verify;

use checks::*;
mod ws;
pub(crate) use ws::*;
mod view;
pub(crate) use view::*;
mod artifacts;
use artifacts::*;

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    App,
    Accounts,
    Version,
    Build,
    Languages,
    Screenshots,
    Metadata,
    Submit,
}

impl Step {
    const ALL: [Step; 8] = [
        Step::App,
        Step::Accounts,
        Step::Version,
        Step::Build,
        Step::Languages,
        Step::Screenshots,
        Step::Metadata,
        Step::Submit,
    ];

    fn group(self) -> &'static str {
        match self {
            Step::App | Step::Accounts => "Setup",
            Step::Version | Step::Build => "Build",
            Step::Languages | Step::Screenshots | Step::Metadata => "Store listing",
            Step::Submit => "Release",
        }
    }

    /// Short label for the sidebar.
    fn label(self) -> &'static str {
        match self {
            Step::App => "App",
            Step::Accounts => "Accounts",
            Step::Version => "Version",
            Step::Build => "Build",
            Step::Languages => "Languages",
            Step::Screenshots => "Screenshots",
            Step::Metadata => "Store text",
            Step::Submit => "Submit",
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Step::App => "App identity",
            Step::Accounts => "Accounts & signing",
            Step::Version => "Version & build numbers",
            Step::Build => "Build",
            Step::Languages => "Languages & source screens",
            Step::Screenshots => "Screenshots",
            Step::Metadata => "Store text",
            Step::Submit => "Submit to stores",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Step::App => "Name, identifiers and platforms. They feed the build scripts and the store lookups.",
            Step::Accounts => "Signing identity and store API keys, checked here so nothing fails halfway through a build or an upload.",
            Step::Version => "What this release is called in the stores, and the build numbers the next builds will use.",
            Step::Build => "Produce the bundles you ship. Builds run in the background — you can move on to other steps meanwhile.",
            Step::Languages => "Pick the store languages, then add the raw app screenshots and their captions for each one.",
            Step::Screenshots => "Choose the sizes and a style, then generate the store-ready images.",
            Step::Metadata => "Descriptions, keywords and release notes for each language, sent with the listing.",
            Step::Submit => "Upload the listing, upload the build, then send it to App Review or a Play track.",
        }
    }
}

/// Steps that apply to this project. Desktop projects have nothing to sign,
/// build or submit here; the Android build of ShipScreen can't run the bash
/// build scripts.
fn visible_steps(platform: &PlatformType) -> Vec<Step> {
    let store_app = !platform.has_desktop();
    Step::ALL
        .into_iter()
        .filter(|s| match s {
            Step::Accounts | Step::Version | Step::Metadata | Step::Submit => store_app,
            Step::Build => store_app && cfg!(not(target_os = "android")),
            _ => true,
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StepStatus {
    Todo,
    Running,
    Attention,
    Done,
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JobKind {
    Screenshots,
    Build,
    AppStore,
    AppStoreBuild,
    AppStoreReview,
    GooglePlay,
    PlayRelease,
}

impl JobKind {
    const ALL: [JobKind; 7] = [
        JobKind::Screenshots,
        JobKind::Build,
        JobKind::AppStore,
        JobKind::AppStoreBuild,
        JobKind::AppStoreReview,
        JobKind::GooglePlay,
        JobKind::PlayRelease,
    ];

    fn label(self) -> &'static str {
        match self {
            JobKind::Screenshots => "Screenshots",
            JobKind::Build => "Build",
            JobKind::AppStore => "App Store listing",
            JobKind::AppStoreBuild => "IPA upload",
            JobKind::AppStoreReview => "App Review",
            JobKind::GooglePlay => "Play listing",
            JobKind::PlayRelease => "Play release",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum JobState {
    Idle,
    Running(String),
    Ok(String),
    Failed(String),
}

// ---------------------------------------------------------------------------
// Things read from disk: store credentials and built bundles
// ---------------------------------------------------------------------------
#[derive(Clone, PartialEq, Debug)]
struct CredCheck {
    label: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Clone, PartialEq, Debug, Default)]
struct Creds {
    app_store: Vec<CredCheck>,
    play: Vec<CredCheck>,
}

impl Creds {
    fn app_store_ok(&self) -> bool {
        self.app_store.iter().all(|c| c.ok)
    }
    fn play_ok(&self) -> bool {
        self.play.iter().all(|c| c.ok)
    }
}

/// A GUI app on Windows opens a console window for every program it starts,
/// unless told not to.
pub(super) fn hide_console(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// `Command::new(…).quiet()`: no console window on Windows.
pub(super) trait Quiet {
    fn quiet(&mut self) -> &mut Self;
}

impl Quiet for std::process::Command {
    fn quiet(&mut self) -> &mut Self {
        hide_console(self);
        self
    }
}

/// Lookup order shared by every job: project `.env`, then `fastlane/.env`,
/// then the process environment.
fn env_lookup(dir: &std::path::Path) -> impl Fn(&str) -> Option<String> {
    let env = load_env(&[dir.join(".env"), dir.join("fastlane").join(".env")]);
    move |key: &str| {
        env.get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .filter(|v| !v.is_empty())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PasswordSource {
    Env,
    Keychain,
}

/// Where the Android upload key comes from. The project's `.env` wins (so
/// existing setups keep working), then the team settings; the password
/// comes from the Keychain. Holds no secret itself.
#[derive(Clone, PartialEq, Debug, Default)]
struct AndroidSigning {
    keystore: Option<PathBuf>,
    keystore_from_env: bool,
    alias: String,
    password: Option<PasswordSource>,
}

/// The key alias for a project: its own setting, else the lowercase slug.
fn key_alias(p: &ProjectState) -> String {
    let own = p.android_key_alias.trim();
    if own.is_empty() {
        p.project_slug.trim().to_lowercase()
    } else {
        own.to_string()
    }
}

/// The keys folder: the team setting, else ~/keys.
fn keys_dir(s: &Settings) -> PathBuf {
    if s.keys_dir.trim().is_empty() {
        signing::default_keys_dir()
    } else {
        signing::expand_home(s.keys_dir.trim())
    }
}

/// Everything the encrypted backup covers.
fn backup_sources(s: &Settings) -> Vec<PathBuf> {
    let dir = keys_dir(s);
    let mut sources = vec![dir.clone()];
    if !s.android_keystore_path.trim().is_empty() {
        let ks = signing::expand_home(s.android_keystore_path.trim());
        if !ks.starts_with(&dir) {
            sources.push(ks);
        }
    }
    sources
}

#[derive(Clone, PartialEq, Debug)]
enum BackupState {
    Never,
    /// Keys changed after the last backup
    Outdated(String),
    Current(String),
}

fn backup_state(s: &Settings) -> BackupState {
    if s.last_backup_at.is_empty() {
        return BackupState::Never;
    }
    let newest = signing::newest_mtime(&backup_sources(s));
    // last_backup_at is "YYYY-MM-DD HH:MM" UTC; compare as seconds.
    let backed_up = parse_utc(&s.last_backup_at);
    match (newest, backed_up) {
        (Some(n), Some(b)) if n > b + std::time::Duration::from_secs(60) => {
            BackupState::Outdated(s.last_backup_at.clone())
        }
        _ => BackupState::Current(s.last_backup_at.clone()),
    }
}

/// "YYYY-MM-DD HH:MM" (UTC) → SystemTime.
fn parse_utc(t: &str) -> Option<SystemTime> {
    let (date, time) = t.split_once(' ')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (hh, mm) = time.split_once(':')?;
    let (hh, mm): (u64, u64) = (hh.parse().ok()?, mm.parse().ok()?);
    // days-from-civil (inverse of checks::utc_now)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(
        SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(days as u64 * 86_400 + hh * 3600 + mm * 60),
    )
}

#[derive(Clone, PartialEq, Debug)]
struct Artifact {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
}

impl Artifact {
    fn file_name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
    fn summary(&self) -> String {
        format!(
            "{} · {:.1} MB · {}",
            self.file_name(),
            self.size as f64 / 1_048_576.0,
            ago(self.modified)
        )
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
struct Artifacts {
    ipa: Option<Artifact>,
    aab: Option<Artifact>,
    apk: Option<Artifact>,
}

/// A path a project setting names: empty is nothing to look at, `~` is the
/// home folder, and a relative path hangs off the project folder.
fn project_path(dir: &std::path::Path, setting: &str) -> Option<std::path::PathBuf> {
    let setting = setting.trim();
    if setting.is_empty() {
        return None;
    }
    let path = signing::expand_home(setting);
    Some(if path.is_absolute() {
        path
    } else {
        dir.join(path)
    })
}

fn ago(t: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

// ---------------------------------------------------------------------------
// Workspace context
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Project View (workspace shell)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Job drawer — one place for every log
// ---------------------------------------------------------------------------

fn log_class(line: &str) -> &'static str {
    let lower = line.to_lowercase();
    if lower.contains("error") || line.starts_with('❌') || line.starts_with('✗') {
        "log-line log-error"
    } else if lower.contains("warning") || line.starts_with('⚠') {
        "log-line log-warn"
    } else if line.starts_with('✅') || line.starts_with('🎉') {
        "log-line log-success"
    } else {
        "log-line"
    }
}

#[cfg(test)]
mod tests;
