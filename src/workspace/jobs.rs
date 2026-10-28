//! Long-running work started from the workspace steps: screenshot generation,
//! build scripts and store uploads.
//!
//! Every task is spawned on the workspace scope (see [`Ws::spawn`]) rather than
//! on the step that started it, so a build keeps running when the user moves on
//! to another step. Progress goes to the job's log signal, which the job drawer
//! renders.

use super::*;
mod publish;
pub(crate) use publish::*;
mod build;
pub(crate) use build::*;
mod generate;
pub(crate) use generate::*;

/// The screenshots written for one screen, as (size label, file), and the
/// URL-encoded path of the one to preview, if any.
type Composited = (Vec<(String, PathBuf)>, Option<String>);

// ---- Logo picker ----
#[cfg(not(target_os = "android"))]
pub(super) fn pick_logo(ws: Ws) {
    let mut proj = ws.proj;
    let proj_dir = ws.dir();
    ws.spawn(async move {
        let file = rfd::AsyncFileDialog::new()
            .set_title("Choose App Logo")
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
            .await;
        if let Some(selected) = file {
            let src = selected.path().to_path_buf();
            // Copy the logo into assets/ inside the project so it survives
            // the original file being moved or deleted later.
            let assets_dir = proj_dir.join("assets");
            let logo_path = if let Some(fname) = src.file_name() {
                let dest = assets_dir.join(fname);
                let _ = tokio::fs::create_dir_all(&assets_dir).await;
                match tokio::fs::copy(&src, &dest).await {
                    Ok(_) => dest,         // use the stable local copy
                    Err(_) => src.clone(), // fallback: use original path
                }
            } else {
                src.clone()
            };
            let mut p = proj.write();
            p.logo_path = Some(logo_path);
            save_project_state(&proj_dir, &p);
        }
    });
}

// rfd has no Android backend (see src/android_saf.rs) — the picker there
// hands back bytes directly rather than a path, so this copies from bytes
// instead of from a source file, but lands in the same place.
#[cfg(target_os = "android")]
pub(super) fn pick_logo(ws: Ws) {
    let mut proj = ws.proj;
    let proj_dir = ws.dir();
    ws.spawn(async move {
        if let Some((fname, bytes)) = android_saf::pick_images(false).await.into_iter().next() {
            let assets_dir = proj_dir.join("assets");
            let _ = tokio::fs::create_dir_all(&assets_dir).await;
            let dest = assets_dir.join(&fname);
            if tokio::fs::write(&dest, &bytes).await.is_ok() {
                let mut p = proj.write();
                p.logo_path = Some(dest);
                save_project_state(&proj_dir, &p);
            }
        }
    });
}

// ---- Source image picker — adds images to the active language ----
#[cfg(not(target_os = "android"))]
pub(super) fn pick_sources(ws: Ws) {
    let mut proj = ws.proj;
    let proj_dir = ws.dir();
    let locale_for_pick = ws.active_locale.read().clone();
    ws.spawn(async move {
        let files = rfd::AsyncFileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_files()
            .await;
        if let Some(selected) = files {
            let mut p = proj.write();
            let srcs = p.locale_sources.entry(locale_for_pick.clone()).or_default();
            for f in selected {
                let path = f.path().to_path_buf();
                if !srcs.contains(&path) {
                    srcs.push(path);
                }
            }
            let n = p.locale_sources[&locale_for_pick].len();
            p.ensure_texts_len(&locale_for_pick, n);
            save_project_state(&proj_dir, &p);
        }
    });
}

// Android has no stable path to reference back to (a content:// URI isn't
// guaranteed to survive past this session), so each picked image is
// copied into the project instead of referenced in place — unlike
// desktop's pick_sources above, which keeps pointing at the original file.
#[cfg(target_os = "android")]
pub(super) fn pick_sources(ws: Ws) {
    let mut proj = ws.proj;
    let proj_dir = ws.dir();
    let locale_for_pick = ws.active_locale.read().clone();
    ws.spawn(async move {
        let picked = android_saf::pick_images(true).await;
        if picked.is_empty() {
            return;
        }
        let sources_dir = proj_dir.join("sources").join(&locale_for_pick);
        let _ = tokio::fs::create_dir_all(&sources_dir).await;

        let mut written = Vec::new();
        for (fname, bytes) in picked {
            let dest = sources_dir.join(&fname);
            if tokio::fs::write(&dest, &bytes).await.is_ok() {
                written.push(dest);
            }
        }

        let mut p = proj.write();
        let srcs = p
            .locale_sources
            .entry(locale_for_pick.clone())
            .or_insert_with(Vec::new);
        for dest in written {
            if !srcs.contains(&dest) {
                srcs.push(dest);
            }
        }
        let n = p.locale_sources[&locale_for_pick].len();
        p.ensure_texts_len(&locale_for_pick, n);
        save_project_state(&proj_dir, &p);
    });
}

// ---- Publish iOS screenshots to App Store Connect ----

// ---- Publish Android screenshots to Google Play via androidpublisher v3 ----

// ---- Run build script ----
// Ensures scripts exist (generates if missing), then runs the chosen one.
/// What a build needs from the project and the team settings, read once when
/// it starts.
#[derive(Clone)]
pub(super) struct BuildInputs {
    dir: PathBuf,
    app_name: String,
    project_slug: String,
    ios_bundle_id: String,
    android_bundle_id: String,
    identity: String,
    profile: String,
    version: String,
    ios_build_number: u32,
    android_version_code: u32,
    keystore_setting: String,
    alias: String,
    logo_path: Option<PathBuf>,
    logo_dest: String,
}

/// The build number a successful `script_name` used is taken in the store's
/// eyes: with auto-increment on, the next release build gets a higher one.
/// A script the developer wrote carries its own numbers, so nothing is bumped
/// after it — ShipScreen's counters would climb past builds that never used
/// them.
pub(super) fn bump_after_build(p: &mut ProjectState, dir: &std::path::Path, script_name: &str) {
    if !p.auto_increment_build || !script_is_ours(dir, script_name) {
        return;
    }
    match script_name {
        "build_ios_distribution.sh" => p.ios_build_number += 1,
        "build_android_release.sh" => p.android_version_code += 1,
        _ => {}
    }
}

// ---- AI Generate ----

// ---- Manual Generate ----
/// The languages selected for generation that have no source screens yet.
pub(super) fn missing_sources(p: &ProjectState) -> Option<String> {
    let empty: Vec<&str> = p
        .locales
        .iter()
        .filter(|loc| p.sources_for(loc).is_empty())
        .map(|l| l.as_str())
        .collect();
    (!empty.is_empty()).then(|| {
        format!(
            "No images for locale(s): {}. Add images in each language tab.",
            empty.join(", ")
        )
    })
}

/// Where a job's output lines go: the job drawer's log in the window, or a
/// plain list when `mcp` runs the job for Claude, where there is no window.
#[derive(Clone)]
pub(crate) enum Log {
    Window(Signal<Vec<String>>),
    Lines(std::rc::Rc<std::cell::RefCell<Vec<String>>>),
}

impl Log {
    pub(crate) fn push(&self, line: String) {
        match self {
            Log::Window(signal) => {
                let mut signal = *signal;
                signal.write().push(line);
            }
            Log::Lines(lines) => lines.borrow_mut().push(line),
        }
    }
}

impl From<Signal<Vec<String>>> for Log {
    fn from(signal: Signal<Vec<String>>) -> Self {
        Log::Window(signal)
    }
}

// ---- Store credentials ----
// Shared by the uploads and the Accounts step's connection tests. Lookup
// order: project .env, fastlane/.env, then the process environment.

fn env_resolver(dir: &std::path::Path) -> impl Fn(&str) -> Option<String> {
    let env = load_env(&[dir.join(".env"), dir.join("fastlane").join(".env")]);
    move |key: &str| {
        env.get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .filter(|v| !v.is_empty())
    }
}

/// The project's App Store Connect API key: (key id, issuer id, .p8 path).
fn asc_key(dir: &std::path::Path) -> Result<(String, String, String), String> {
    let resolve = env_resolver(dir);
    let missing = |key: &str| format!("Missing {key} in .env");
    let key_id = resolve("APP_STORE_CONNECT_API_KEY_KEY_ID")
        .ok_or_else(|| missing("APP_STORE_CONNECT_API_KEY_KEY_ID"))?;
    let issuer_id = resolve("APP_STORE_CONNECT_API_KEY_ISSUER_ID")
        .ok_or_else(|| missing("APP_STORE_CONNECT_API_KEY_ISSUER_ID"))?;
    let key_path = resolve("APP_STORE_CONNECT_API_KEY_KEY_FILEPATH")
        .ok_or_else(|| missing("APP_STORE_CONNECT_API_KEY_KEY_FILEPATH"))?;
    Ok((key_id, issuer_id, key_path))
}

/// Mint an App Store Connect JWT from the project's API key.
async fn asc_auth(dir: &std::path::Path) -> Result<String, String> {
    let (key_id, issuer_id, key_path) = asc_key(dir)?;
    let p8_pem = tokio::fs::read_to_string(&key_path)
        .await
        .map_err(|e| format!("Cannot read .p8 key at {key_path}: {e}"))?;
    asc_mint_jwt(&key_id, &issuer_id, &p8_pem).map_err(|e| format!("JWT error: {e}"))
}

// ---- Connection tests (Accounts step) ----

// ---- Release history ----

fn record_release(ws: Ws, platform: &str, version: &str, build: u32, action: &str) {
    let record = ReleaseRecord {
        at: utc_now(),
        platform: platform.to_string(),
        version: version.to_string(),
        build,
        action: action.to_string(),
    };
    ws.update(|p| {
        p.releases.insert(0, record);
        p.releases.truncate(100);
    });
}

// ---- iOS: upload the IPA ----

/// The iOS build to submit: the last one uploaded from here, else the IPA on disk.
fn ios_build_to_submit(ws: Ws) -> Option<(String, u32)> {
    let last_uploaded = ws
        .proj
        .read()
        .releases
        .iter()
        .find(|r| r.platform == "ios" && r.action == "Uploaded build")
        .map(|r| (r.version.clone(), r.build));
    last_uploaded.or_else(|| {
        ws.artifacts
            .read()
            .ipa
            .as_ref()
            .and_then(|a| stores::ipa_version(&a.path))
    })
}

// ---- iOS: attach the build and submit for review ----

// ---- Android: upload the AAB and release it to a track ----

// ---- Apple signing repair (Accounts step) ----

// ---- What the stores currently show (Submit step) ----
