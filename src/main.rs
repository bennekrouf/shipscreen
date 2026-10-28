// A release build on Windows is a GUI app: no console window behind it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use ab_glyph::{FontRef, PxScale};
use dioxus::desktop::wry::http::{Response, StatusCode};
use dioxus::desktop::wry::RequestAsyncResponder;
use dioxus::desktop::{use_asset_handler, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod kit;
mod licence;
mod pro;
mod toolpath;
mod workspace;

use workspace::ProjectView;
#[cfg(target_os = "android")]
mod android_saf;
mod build_scripts;
use build_scripts::*;
mod scaffold;
use scaffold::*;
mod google_play;
use google_play::*;
mod app_store_connect;
use app_store_connect::*;
mod imaging;
use imaging::*;
mod project;
use project::*;
mod picker;
use picker::*;
mod app;
use app::*;
mod settings;
use settings::*;

// Embedded rather than shipped as a manganis asset: the bundled `.app` /
// `.msi` / `.deb` only carry `assets/` if the packaging step copies them in,
// and when it silently does not (as in the v0.1.2 dmg) the webview 404s the
// stylesheet and renders raw unstyled DOM. Baking it into the binary — like
// ROBOTO_FONT and the window icon — makes that failure mode impossible.
const MAIN_CSS: &str = include_str!("../assets/main.css");
const ROBOTO_FONT: &[u8] = include_bytes!("../assets/Roboto-Bold.ttf");
/// The first screen's logo, for the same reason: the Windows build ships no `assets/` at all.
/// Twice the size it is shown at, for high-density screens.
const LOGO_MARK_PNG: &[u8] = include_bytes!("../assets/logo-mark.png");

/// The first screen's logo as a `data:` address, made once.
fn logo_mark_src() -> &'static str {
    use base64::Engine;
    static SRC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(LOGO_MARK_PNG)
        )
    })
}

// ---------------------------------------------------------------------------
// Locales supported by abjad (fastlane locale code -> display name)
// ---------------------------------------------------------------------------
const LOCALES: &[(&str, &str)] = &[
    ("ar-SA", "Arabic"),
    ("en-US", "English"),
    ("fr-FR", "French"),
    ("hi", "Hindi"),
    ("id", "Indonesian"),
    ("ms", "Malay"),
    ("sq", "Albanian"),
    ("tr", "Turkish"),
    ("ur", "Urdu"),
    ("zh-Hans", "Chinese (Simplified)"),
];

// iOS target -> fastlane device folder name
const IOS_TARGETS: &[(&str, &str, u32, u32)] = &[
    ("ios_iphone_69", "iPhone 6.9\" Display", 1320, 2868),
    ("ios_iphone_67", "iPhone 6.7\" Display", 1290, 2796),
    ("ios_iphone_65", "iPhone 6.5\" Display", 1242, 2688),
    ("ios_ipad_129", "iPad Pro (12.9-inch)", 2048, 2732),
];

// Android targets still go to output/ inside the project
const ANDROID_TARGETS: &[(&str, &str, u32, u32)] = &[
    ("android_phone", "Android Phone", 1080, 2340),
    ("android_feature", "Android Feature", 1024, 500),
];

// Desktop screenshot targets (used when platform_type == Desktop)
const DESKTOP_TARGETS: &[(&str, &str, u32, u32)] = &[
    ("desktop_mac", "macOS (1280×800)", 1280, 800),
    ("desktop_win", "Windows (1280×720)", 1280, 720),
    ("desktop_wide", "Widescreen (1920×1080)", 1920, 1080),
];

// ---------------------------------------------------------------------------
// Global settings (gear popup)
// ---------------------------------------------------------------------------

fn default_phone_style() -> String {
    "modern smartphone".to_string()
}

fn default_ios_short_version() -> String {
    "1.0".to_string()
}

fn global_config_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        // Still the old name: it lists every project, and the app was AppScreens
        // when it was first written.
        .join("appscreens");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("settings.json")
}

fn load_settings() -> Settings {
    std::fs::read_to_string(global_config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(s: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(global_config_path(), json);
    }
}

// ---------------------------------------------------------------------------
// Platform type
// ---------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum PlatformType {
    Desktop,
    Ios,
    Android,
    #[default]
    IosAndroid,
}

impl PlatformType {
    fn label(&self) -> &'static str {
        match self {
            PlatformType::Desktop => "Desktop",
            PlatformType::Ios => "iOS",
            PlatformType::Android => "Android",
            PlatformType::IosAndroid => "iOS + Android",
        }
    }
    fn has_ios(&self) -> bool {
        matches!(self, PlatformType::Ios | PlatformType::IosAndroid)
    }
    fn has_android(&self) -> bool {
        matches!(self, PlatformType::Android | PlatformType::IosAndroid)
    }
    fn has_desktop(&self) -> bool {
        matches!(self, PlatformType::Desktop)
    }
}

// ---------------------------------------------------------------------------
// Per-project state (saved as <project_dir>/shipscreen.json)
// ---------------------------------------------------------------------------

/// Store text for one locale. Limits are the stores' own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
struct StoreText {
    /// Both stores (4000)
    #[serde(default)]
    description: String,
    /// App Store keywords, comma-separated (100)
    #[serde(default)]
    keywords: String,
    /// App Store promotional text (170)
    #[serde(default)]
    promo_text: String,
    /// "What's new" — App Store (4000) and Play release notes (500)
    #[serde(default)]
    whats_new: String,
    /// Google Play short description (80)
    #[serde(default)]
    short_description: String,
}

/// One step of shipping a release, for the Submit step's history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ReleaseRecord {
    /// UTC, "YYYY-MM-DD HH:MM"
    at: String,
    /// "ios" / "android"
    platform: String,
    version: String,
    build: u32,
    /// e.g. "Uploaded build", "Submitted for review", "Released to internal (draft)"
    action: String,
}

fn default_play_track() -> String {
    "internal".to_string()
}
fn default_play_status() -> String {
    "draft".to_string()
}

fn default_logo_dest() -> String {
    "assets/icon.png".to_string()
}

fn default_true() -> bool {
    true
}
fn default_locale() -> String {
    "en-US".to_string()
}
fn default_locales() -> Vec<String> {
    vec!["en-US".to_string()]
}
fn default_ios_targets() -> Vec<bool> {
    vec![true; IOS_TARGETS.len()]
}
fn default_android_targets() -> Vec<bool> {
    vec![true; ANDROID_TARGETS.len()]
}
fn default_desktop_targets() -> Vec<bool> {
    vec![true; DESKTOP_TARGETS.len()]
}

/// `shipscreen.json`, or `appscreens.json` in a project set up before the app
/// was renamed ShipScreen. That file is kept rather than renamed: it sits in the
/// developer's own repository, and its .gitignore names it.
fn project_state_path(project_dir: &Path) -> PathBuf {
    let path = project_dir.join("shipscreen.json");
    let legacy = project_dir.join("appscreens.json");
    if !path.exists() && legacy.exists() {
        legacy
    } else {
        path
    }
}

/// Each project's state file's modification time as this process last read or
/// wrote it. Two processes write it — the window, and `shipscreen mcp` when
/// Claude drives it — so a different time means the other one did.
fn state_seen(
) -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, Option<std::time::SystemTime>>> {
    static SEEN: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, Option<std::time::SystemTime>>>,
    > = std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default)
}

fn state_mtime(project_dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(project_state_path(project_dir))
        .and_then(|m| m.modified())
        .ok()
}

fn note_state(project_dir: &Path) {
    let mut seen = state_seen().lock().unwrap_or_else(|e| e.into_inner());
    seen.insert(project_dir.to_path_buf(), state_mtime(project_dir));
}

/// Whether another process wrote the project's state since this one last read
/// or wrote it. Notes the current state, so each change is reported once.
fn project_state_changed_elsewhere(project_dir: &Path) -> bool {
    let now = state_mtime(project_dir);
    let mut seen = state_seen().lock().unwrap_or_else(|e| e.into_inner());
    seen.insert(project_dir.to_path_buf(), now)
        .is_some_and(|before| before != now)
}

fn load_project_state(project_dir: &Path) -> ProjectState {
    let text = std::fs::read_to_string(project_state_path(project_dir));
    note_state(project_dir);
    let mut state = text
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(ProjectState::with_defaults);
    state.normalize_targets();
    // Remember whether we have legacy paths to migrate.
    let had_legacy_paths = !state.source_paths.is_empty();
    state.migrate_legacy();
    // If legacy source_paths were migrated, persist the cleaned state immediately.
    // This ensures source_paths is saved as [] so future loads won't re-add images
    // that the user has already deleted from locale_sources.
    if had_legacy_paths {
        save_project_state(project_dir, &state);
    }
    state
}

fn save_project_state(project_dir: &Path, state: &ProjectState) {
    match serde_json::to_string_pretty(state) {
        Ok(json) => {
            if let Err(e) = std::fs::write(project_state_path(project_dir), &json) {
                eprintln!(
                    "[ShipScreen] ⚠️  Failed to save project state to {:?}: {e}",
                    project_state_path(project_dir)
                );
            }
            note_state(project_dir);
        }
        Err(e) => {
            eprintln!("[ShipScreen] ⚠️  Failed to serialize project state: {e}");
        }
    }
}

const MAX_THEME_HISTORY: usize = 20;
const MAX_RECENT_PROJECTS: usize = 8;

// ---------------------------------------------------------------------------
// Build script templates
// Only the top variable block differs between projects — the rest is preserved
// exactly as the working originals.
// ---------------------------------------------------------------------------

/// First lines of every script ShipScreen writes; how it knows which ones it may replace.
const SCRIPT_MARKER: &str = "# Written by ShipScreen before each build — edits here are replaced.";

/// A script this app wrote, under either name: it was AppScreens before ShipScreen,
/// and the scripts it wrote then are still its own to replace.
fn written_by_us(text: &str) -> bool {
    text.contains("ShipScreen") || text.contains("AppScreens")
}

/// Whether the built-in build scripts fit this project at all: they run `dx`,
/// so they need a Cargo package with Dioxus in it. An Xcode or Gradle project
/// brings its own script instead (see the README), and ShipScreen writes
/// nothing of its own into it.
fn is_dioxus_project(dir: &std::path::Path) -> bool {
    dir.join("Dioxus.toml").is_file()
        || std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|t| t.contains("dioxus"))
}

/// What a build script may be called, in the order this platform prefers:
/// bash is not the thing to reach for on Windows, so a project can bring
/// something `cmd` or PowerShell runs instead.
#[cfg(windows)]
const SCRIPT_KINDS: [&str; 4] = ["cmd", "bat", "ps1", "sh"];
#[cfg(not(windows))]
const SCRIPT_KINDS: [&str; 2] = ["sh", "ps1"];

/// The script that will run for `name` (always a `.sh` name inside
/// ShipScreen): the first one the project has, in this platform's order.
fn build_script(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let stem = name.strip_suffix(".sh").unwrap_or(name);
    SCRIPT_KINDS
        .iter()
        .map(|kind| dir.join(format!("{stem}.{kind}")))
        .find(|path| path.is_file())
}

/// Whether the script that runs for `name` is one ShipScreen wrote, so the
/// build used the version and build number from the Version step. A script
/// the developer wrote carries its own numbers — and anything but a `.sh` is
/// theirs, since those are the only ones ShipScreen writes. Nothing there at
/// all means ShipScreen's internal Android build did the work.
fn script_is_ours(dir: &std::path::Path, name: &str) -> bool {
    match build_script(dir, name) {
        None => true,
        Some(path) if path.extension().is_some_and(|e| e != "sh") => false,
        Some(path) => std::fs::read_to_string(path).is_ok_and(|t| written_by_us(&t)),
    }
}

// ---------------------------------------------------------------------------
// Scaffold a brand-new Dioxus project from ShipScreen (no pre-run scaffolder needed)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// App phase
// ---------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
enum AppPhase {
    Idle,
    GeneratingAi,
    GeneratingManual,
    Resizing,
    Done,
    Error(String),
}

// Build script phase
#[derive(Clone, Debug, PartialEq)]
enum BuildPhase {
    Idle,
    Running(String), // which script is running
    Success(String),
    Error(String),
}

// Publish phase (iOS / App Store Connect)
#[derive(Clone, Debug, PartialEq)]
enum PublishPhase {
    Idle,
    Running,
    Success,
    Error(String),
}

// Android publish phase (Google Play via androidpublisher v3)
#[derive(Clone, Debug, PartialEq)]
enum AndroidPublishPhase {
    Idle,
    Running,
    Success,
    Error(String),
}

/// The native title bar, as in every mayorana app: name, version and licence.
fn window_title(status: &licence::Status) -> String {
    format!(
        "ShipScreen {}{}",
        env!("CARGO_PKG_VERSION"),
        licence::title_badge(status)
    )
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
fn main() {
    kit::init();
    // `shipscreen mcp`: the projects over stdio for an MCP client, and no
    // window. Before the log subscriber, which writes to stdout: there,
    // anything but the protocol is a corrupt message.
    #[cfg(not(target_os = "android"))]
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        toolpath::init();
        return workspace::mcp::serve();
    }
    tracing_subscriber::fmt::init();
    // Before any thread starts: PATH is read by every command ShipScreen runs.
    toolpath::init();

    let config = dioxus::desktop::Config::new()
        // Injected into index.html's <head> before first paint, so there is no
        // flash of unstyled content and no runtime asset lookup.
        //
        // The viewport meta goes in with it: without it the webview lays the
        // page out in a 980px virtual viewport on Android, so every
        // `@media (max-width: …)` rule in main.css silently never matches on
        // the one platform that needs them.
        .with_custom_head(format!(
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0, viewport-fit=cover\">\
             <style>{MAIN_CSS}</style>"
        ))
        .with_window(
            WindowBuilder::new()
                .with_title(window_title(&licence::current()))
                .with_window_icon(make_icon())
                .with_inner_size(LogicalSize::new(1020.0, 860.0))
                .with_focused(true)
                .with_decorations(true)
                .with_transparent(false),
        );

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

// ── Procedural window icon (mirrors ais-runner's style) ─────────────────────
// Circular mask + blue→purple gradient + white symbol. Drawn in pure Rust so
// the binary stays self-contained — no PNG asset to ship.

// ---------------------------------------------------------------------------
// Root App
// ---------------------------------------------------------------------------
// dark-light has no Android backend (see Cargo.toml) — there's no per-app
// system light/dark signal to read there the way desktop OSes expose one, so
// this just keeps the app on its existing light-by-default behavior (the
// desktop check below also treats "can't tell" as light, via `!= Dark`).
#[cfg(not(target_os = "android"))]
fn system_prefers_light() -> bool {
    dark_light::detect() != dark_light::Mode::Dark
}
#[cfg(target_os = "android")]
fn system_prefers_light() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Icons
//
// These replaced the emoji that used to label buttons and tabs (🚀 Publish,
// 🔨 Build, 📱 Build iOS IPA, 🐛 Build APK, …). Emoji render as full-colour
// glyphs in an otherwise monochrome indigo UI, so they were the loudest thing
// on screen while carrying no meaning, and they vary per platform — a 🐛 on a
// build button reads as "report a bug". Stroke geometry on a 24×24 grid,
// coloured by `currentColor` so each icon inherits whatever accent its control
// already owns. Sizing and stroke live in `.icon` in main.css.
// ---------------------------------------------------------------------------
macro_rules! icon {
    ($name:ident, $($d:expr),+ $(,)?) => {
        fn $name() -> Element {
            rsx! {
                svg { class: "icon", view_box: "0 0 24 24", "aria-hidden": "true",
                    $( path { d: $d } )+
                }
            }
        }
    };
}

icon!(
    icon_sparkle,
    "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z"
);
icon!(
    icon_image,
    "M4 4h16v16H4z",
    "M4 16l4.5-4.5 3 3 3.5-3.5L20 15"
);
icon!(
    icon_upload,
    "M12 16V4",
    "M7 9l5-5 5 5",
    "M4 16v3a2 2 0 002 2h12a2 2 0 002-2v-3"
);
icon!(icon_phone, "M6 2h12v20H6z", "M11 18.5h2");
icon!(
    icon_package,
    "M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z",
    "M3.3 7l8.7 5 8.7-5",
    "M12 22V12",
);
icon!(
    icon_download,
    "M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4",
    "M7 10l5 5 5-5",
    "M12 15V3"
);
icon!(icon_play, "M6 3l14 9-14 9z");
icon!(icon_monitor, "M3 4h18v12H3z", "M8 20h8", "M12 16v4");
icon!(
    icon_alert,
    "M10.3 3.9L2 18a2 2 0 001.7 3h16.6a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z",
    "M12 9v4.5",
    "M12 17.2h.01",
);
icon!(icon_close, "M18 6L6 18", "M6 6l12 12");
icon!(icon_plus, "M12 5v14", "M5 12h14");
icon!(icon_chevron_left, "M15 5l-7 7 7 7");
icon!(icon_chevron_right, "M9 5l7 7-7 7");
icon!(icon_check, "M20 6L9 17l-5-5");
icon!(
    icon_folder_plus,
    "M21 19a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h4.5l2 3H19a2 2 0 012 2z",
    "M12 11.5v5",
    "M9.5 14h5"
);

// Android has no folder-browse equivalent (see src/android_saf.rs) — every
// project the user could open was created through this same picker, on the
// one fixed app-private root, so it's already in Recent Projects. Nothing
// external to browse to, so this renders nothing there.
//
// (`#[cfg]` doesn't parse directly on an rsx! element, hence pulling this out
// into its own function rather than cfg'ing the `button {}` in place.)

// Meaningless on Android, where new_parent is always already Some (see its
// #[cfg]'d initializer in ProjectPicker).

// ---------------------------------------------------------------------------
// Project Picker (shown on launch)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Settings popup (gear)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// fal.ai API
// ---------------------------------------------------------------------------
#[derive(Serialize)]
struct FalTextToImageRequest {
    prompt: String,
    image_size: FalImageSize,
    num_inference_steps: u32,
    guidance_scale: f32,
    num_images: u32,
    output_format: String,
}
#[derive(Serialize)]
struct FalImageSize {
    width: u32,
    height: u32,
}
#[derive(Deserialize, Debug)]
struct FalResponse {
    images: Vec<FalImage>,
}
#[derive(Deserialize, Debug)]
struct FalImage {
    url: String,
}

// ---------------------------------------------------------------------------
// Placeholder detection
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Composite
// ---------------------------------------------------------------------------

fn fallback_placement(frame_w: u32, frame_h: u32) -> (u32, u32, u32, u32) {
    let sw = (frame_w as f64 * 0.55) as u32;
    let sh = (frame_h as f64 * 0.72) as u32;
    ((frame_w - sw) / 2, (frame_h - sh) / 2, sw, sh)
}

async fn download_image(url: &str) -> Result<Vec<u8>, String> {
    reqwest::get(url)
        .await
        .map_err(|e| format!("Download failed: {e}"))?
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read bytes failed: {e}"))
}

// ---------------------------------------------------------------------------
// Resize to targets
// ---------------------------------------------------------------------------
/// Where a generation run saves its images, and which sizes are ticked
/// (one bool per entry of IOS_TARGETS / ANDROID_TARGETS / DESKTOP_TARGETS;
/// an empty list exports nothing for that platform).
#[derive(Clone)]
struct ExportTargets {
    /// `fastlane/screenshots/ios`
    ios_dir: PathBuf,
    /// `fastlane/metadata/android`
    android_root: PathBuf,
    /// `fastlane/screenshots/desktop`
    desktop_dir: PathBuf,
    ios: Vec<bool>,
    android: Vec<bool>,
    desktop: Vec<bool>,
}

/// Composite an RGBA image onto a white background, returning an opaque RGB-equivalent RgbaImage.
/// App Store Connect rejects PNGs that carry an alpha channel (IMAGE_ALPHA_NOT_ALLOWED).
fn flatten_alpha_onto_white(src: &RgbaImage) -> image::RgbImage {
    let (w, h) = src.dimensions();
    let mut out = image::RgbImage::new(w, h);
    for (x, y, pixel) in src.enumerate_pixels() {
        let a = pixel[3] as f32 / 255.0;
        let r = (pixel[0] as f32 * a + 255.0 * (1.0 - a)) as u8;
        let g = (pixel[1] as f32 * a + 255.0 * (1.0 - a)) as u8;
        let b = (pixel[2] as f32 * a + 255.0 * (1.0 - a)) as u8;
        out.put_pixel(x, y, image::Rgb([r, g, b]));
    }
    out
}

fn fill_and_crop(src: &RgbaImage, target_w: u32, target_h: u32) -> RgbaImage {
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let scale = (target_w as f64 / sw).max(target_h as f64 / sh);
    let new_w = (sw * scale).round().max(1.0) as u32;
    let new_h = (sh * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(src, new_w, new_h, FilterType::Lanczos3);
    let crop_x = (new_w.saturating_sub(target_w)) / 2;
    let crop_y = (new_h.saturating_sub(target_h)) / 2;
    image::imageops::crop_imm(&resized, crop_x, crop_y, target_w, target_h).to_image()
}

// ---------------------------------------------------------------------------
// Color utilities
// ---------------------------------------------------------------------------
fn parse_hex_color(hex: &str) -> Option<Rgba<u8>> {
    let hex = hex.trim_start_matches('#');
    let rgb = hex::decode(hex).ok()?;
    if rgb.len() >= 3 {
        Some(Rgba([rgb[0], rgb[1], rgb[2], 255]))
    } else {
        None
    }
}

fn lighten_color(color: Rgba<u8>) -> Rgba<u8> {
    let Rgba([r, g, b, a]) = color;
    let mix = |c: u8| -> u8 { ((c as f32 * 0.7) + (255.0 * 0.3)) as u8 };
    Rgba([mix(r), mix(g), mix(b), a])
}

fn get_contrast_color(bg: Rgba<u8>) -> Rgba<u8> {
    let Rgba([r, g, b, _]) = bg;
    let l = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    if l > 128.0 {
        Rgba([0, 0, 0, 255])
    } else {
        Rgba([255, 255, 255, 255])
    }
}

// ---------------------------------------------------------------------------
// Strip ANSI escape codes from terminal output for clean log display
// ---------------------------------------------------------------------------
fn strip_ansi(s: &str) -> String {
    // Matches ESC[ ... m  (SGR) and other common escape sequences
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume the escape sequence: ESC followed by '[' then up to final byte in 0x40–0x7E
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                for nc in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&nc) {
                        break;
                    }
                }
            } else {
                // Non-CSI escape: consume one more char
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Google Play androidpublisher v3 helpers
// ---------------------------------------------------------------------------

/// Silently delete (abort) an edit — used for cleanup on error.
async fn google_play_delete_edit(
    token: &str,
    package_name: &str,
    edit_id: &str,
) -> Result<(), String> {
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{package_name}/edits/{edit_id}"
    );
    let _ = reqwest::Client::new()
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await;
    Ok(())
}

// ---------------------------------------------------------------------------
// App Store Connect API v1 helpers
// ---------------------------------------------------------------------------
