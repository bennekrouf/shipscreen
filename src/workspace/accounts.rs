//! 2 · Accounts & signing: store credentials, Apple signing (identity,
//! certificate, profile), the Android upload key, secrets tracked by git and
//! the encrypted backup of all signing material.

use super::steps::{check_row, job_status_line};
use super::*;
mod apple;
pub(crate) use apple::*;
mod play;
pub(crate) use play::*;
mod backup;
pub(crate) use backup::*;

/// Signing identities from the login keychain, Distribution/Development only.
fn discover_identities() -> Vec<String> {
    std::process::Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|out| {
            out.lines()
                .filter_map(|line| {
                    // Lines look like:  1) HASH "Identity Name (TEAMID)"
                    let q = line.find('"')?;
                    let rest = &line[q + 1..];
                    let end = rest.rfind('"')?;
                    Some(rest[..end].to_string())
                })
                .filter(|id| {
                    id.starts_with("Apple Distribution") || id.starts_with("Apple Development")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

// rfd has no Android backend; there is nothing to sign there anyway.
#[cfg(not(target_os = "android"))]
async fn pick_file(title: &str, filter: (&str, &[&str])) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter.0, filter.1)
        .pick_file()
        .await
        .map(|f| f.path().to_path_buf())
}
#[cfg(target_os = "android")]
async fn pick_file(_title: &str, _filter: (&str, &[&str])) -> Option<PathBuf> {
    None
}

#[cfg(not(target_os = "android"))]
async fn save_file(title: &str, name: &str) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(name)
        .save_file()
        .await
        .map(|f| f.path().to_path_buf())
}
#[cfg(target_os = "android")]
async fn save_file(_title: &str, _name: &str) -> Option<PathBuf> {
    None
}

/// Connection-test button plus its latest result.
fn connection_test(state: JobState, on_test: impl FnMut(MouseEvent) + 'static) -> Element {
    let running = matches!(state, JobState::Running(_));
    rsx! {
        div { class: "conn-test",
            button {
                class: "btn btn-sm",
                disabled: running,
                onclick: on_test,
                if running {
                    span { class: "spinner spinner-dark" }
                    " Testing…"
                } else {
                    "Test connection"
                }
            }
            div { class: "publish-status-row", {job_status_line(state, "Not tested yet")} }
        }
    }
}

/// A password field bound to a local signal.
fn password_input(mut value: Signal<String>, placeholder: &'static str) -> Element {
    rsx! {
        input {
            class: "text-input",
            r#type: "password",
            autocomplete: "new-password",
            placeholder: "{placeholder}",
            value: "{value}",
            oninput: move |e: Event<FormData>| value.set(e.value()),
        }
    }
}

#[component]
pub(super) fn AccountsStep() -> Element {
    let ws = use_context::<Ws>();
    let mut refresh = ws.refresh;
    let plat = ws.proj.read().platform_type.clone();

    // .env files, profiles, keys and git may have changed since the last look.
    use_effect(move || refresh.with_mut(|n| *n += 1));

    rsx! {
        if plat.has_ios() {
            AppleCard {}
        }
        if plat.has_android() {
            PlayCard {}
            UploadKeyCard {}
        }
        SecretsCard {}
        BackupCard {}
        LostMacCard {}

        button {
            class: "btn",
            onclick: move |_| refresh.with_mut(|n| *n += 1),
            "Check everything again"
        }
    }
}

// ---------------------------------------------------------------------------
// Apple
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Google Play
// ---------------------------------------------------------------------------

/// A copy-to-clipboard button for a value the user pastes elsewhere.
fn copy_button(value: String, mut state: Signal<JobState>) -> Element {
    rsx! {
        button {
            class: "btn btn-sm",
            onclick: move |_| {
                let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&value).unwrap_or_default());
                document::eval(&js);
                state.set(JobState::Ok("Copied".into()));
            },
            "Copy"
        }
    }
}

// ---------------------------------------------------------------------------
// Android upload key
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Secrets tracked by git
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Backup
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// If this Mac is lost
// ---------------------------------------------------------------------------

/// Google service-account key file → (project id, key id prefix).
fn service_account_ids(path: &str) -> Option<(String, String)> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(signing::expand_home(path)).ok()?).ok()?;
    Some((
        v["project_id"].as_str()?.to_string(),
        v["private_key_id"].as_str()?.chars().take(12).collect(),
    ))
}
