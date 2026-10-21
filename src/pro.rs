//! ShipScreen Pro: the window, and the free version's one app.
//!
//! Without Pro, ShipScreen works on one app (project folder): the first one
//! opened takes the free slot, kept in `free_app.json` beside the licence.
//! Opening another asks for Pro, or offers to make that one the free app
//! instead — the same deal as GitAgent's free repositories.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;

use crate::licence::{self, FreeError, Status};

const FREE_APP_FILE: &str = "free_app.json";

/// Shared by every part of the UI that can open the window or needs to know
/// whether the app is licensed. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct Pro {
    pub open: Signal<bool>,
    pub status: Signal<Status>,
    /// The app someone tried to open while the free slot holds another one.
    pub pending: Signal<Option<PathBuf>>,
}

/// The app the free version works on, once one has been opened.
pub fn free_app() -> Option<PathBuf> {
    let json = std::fs::read_to_string(licence::dir().join(FREE_APP_FILE)).ok()?;
    serde_json::from_str(&json).ok()
}

fn set_free_app(dir: &Path) {
    let _ = std::fs::create_dir_all(licence::dir());
    if let Ok(json) = serde_json::to_string(dir) {
        let _ = std::fs::write(licence::dir().join(FREE_APP_FILE), json);
    }
}

/// The same folder, however it was reached (a symlink, a trailing slash).
fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[derive(Debug, PartialEq)]
enum Decision {
    Open,
    /// Open it, and it becomes the free app.
    TakeSlot,
    Locked,
}

fn decide(status: &Status, slot: Option<&Path>, dir: &Path) -> Decision {
    if status.unlimited() {
        return Decision::Open;
    }
    match slot {
        None => Decision::TakeSlot,
        Some(f) if same_folder(f, dir) => Decision::Open,
        Some(_) => Decision::Locked,
    }
}

/// Whether `dir` may be opened now, taking the free slot for it if the slot
/// is still empty.
pub fn may_open(status: &Status, dir: &Path) -> bool {
    match decide(status, free_app().as_deref(), dir) {
        Decision::Open => true,
        Decision::TakeSlot => {
            set_free_app(dir);
            true
        }
        Decision::Locked => false,
    }
}

fn folder_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

fn open_url(url: &str) {
    use std::process::Command;
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        Command::new("cmd").args(["/c", "start", "", url]).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
}

/// "Get Pro…" while free, "✦ PRO" once licensed; opens the window either way.
#[component]
pub fn ProButton() -> Element {
    let mut pro = use_context::<Pro>();
    let status = pro.status.read().clone();
    let (label, class, title) = match &status {
        Status::Pro(a) => (
            "✦ PRO",
            "pro-btn active",
            format!("ShipScreen Pro, {} plan for {}", a.plan.name, a.email),
        ),
        Status::NotPro(a) if a.is_free() => (
            "Get Pro…",
            "pro-btn",
            format!("Free: {} AI credits left this month. The free version works on one app; Pro on any number", a.credits.left),
        ),
        Status::NotPro(_) | Status::Offline(_) | Status::Unknown(_) => (
            "✦ RENEW PRO",
            "pro-btn renew",
            "Your licence doesn't make this ShipScreen Pro right now".to_owned(),
        ),
        Status::Free => (
            "Get Pro…",
            "pro-btn",
            "The free version works on one app; Pro on any number".to_owned(),
        ),
    };
    rsx! {
        button {
            class,
            title,
            onclick: move |_| {
                pro.pending.set(None);
                pro.open.set(true);
            },
            "{label}"
        }
    }
}

/// The Pro window. `on_open` opens a project folder; it is called with the
/// app that was waiting once Pro is activated or it becomes the free app.
#[component]
pub fn ProWindow(on_open: EventHandler<PathBuf>) -> Element {
    let mut pro = use_context::<Pro>();
    let mut key = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);
    let mut free_email = use_signal(String::new);
    let mut free_note = use_signal(|| Option::<String>::None);
    let mut free_hidden = use_signal(|| false);
    if !(pro.open)() {
        return rsx! {};
    }
    let status = pro.status.read().clone();
    let asks_number =
        matches!(status, Status::Free | Status::Unknown(_)) || status.free_credits().is_some();
    let offers_free = matches!(status, Status::Free | Status::Unknown(_)) && !free_hidden();
    let pending = pro.pending.read().clone();
    let free = free_app();
    let mut close = move || {
        pro.open.set(false);
        pro.pending.set(None);
        error.set(None);
    };
    let mut activate = move || {
        let pasted = key.read().clone();
        busy.set(true);
        spawn(async move {
            match licence::activate(&pasted).await {
                Ok(s) => {
                    let unlocked = s.unlimited();
                    pro.status.set(s);
                    key.set(String::new());
                    error.set(None);
                    if unlocked {
                        if let Some(dir) = pro.pending.take() {
                            pro.open.set(false);
                            on_open.call(dir);
                        }
                    }
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let mut check_again = move || {
        busy.set(true);
        spawn(async move {
            pro.status.set(licence::refresh(true).await);
            busy.set(false);
        });
    };
    let mut ask_free = move || {
        let email = free_email.read().clone();
        busy.set(true);
        spawn(async move {
            match licence::request_free_credits(&email).await {
                Ok(message) => {
                    free_note.set(Some(format!(
                        "{message} Paste the licence number above when it arrives."
                    )));
                    error.set(None);
                }
                Err(FreeError::Unavailable) => free_hidden.set(true),
                Err(FreeError::Refused(e)) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let mut manage = move |number: String| {
        busy.set(true);
        spawn(async move {
            match licence::portal_url(&number).await {
                Ok(url) => {
                    open_url(&url);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let free_name = free.as_deref().map(folder_name);
    let pending_name = pending.as_deref().map(folder_name);

    rsx! {
        div { class: "settings-backdrop", onclick: move |_| close() }
        div { class: "settings-popup pro-popup",
            div { class: "settings-header",
                h2 { "ShipScreen Pro" }
            }
            match &status {
                Status::Pro(a) => rsx! {
                    p { class: "pro-ok",
                        "Pro is active on this computer: "
                        strong { "{a.plan.name}" }
                        " plan for "
                        strong { "{a.email}" }
                        "."
                    }
                    p { class: "settings-hint",
                        "{a.credits.left} AI credits left this month. {a.renewal()}."
                    }
                },
                Status::NotPro(a) if a.is_free() => rsx! {
                    p {
                        "Free — "
                        strong { "{a.credits.left} AI credits" }
                        " left this month, for "
                        strong { "{a.email}" }
                        ". {a.renewal()}."
                    }
                },
                Status::NotPro(a) if !a.active => rsx! {
                    p {
                        "The subscription for "
                        strong { "{a.email}" }
                        " isn't active, so ShipScreen runs as the free version. Renew it to have Pro again."
                    }
                },
                Status::NotPro(a) => rsx! {
                    p {
                        "The "
                        strong { "{a.plan.name}" }
                        " plan doesn't include ShipScreen, so it runs as the free version. A plan with ShipScreen makes it Pro."
                    }
                },
                Status::Offline(a) => rsx! {
                    p {
                        "ShipScreen hasn't been able to check the licence for "
                        strong { "{a.email}" }
                        " for a week, so it runs as the free version until it can. Check the connection, then check again."
                    }
                },
                Status::Unknown(_) => rsx! {
                    p { "The licence number on this computer isn't known. Check it against the email, or paste it again." }
                },
                Status::Free => rsx! {},
            }
            if !status.unlimited() {
                if let (Some(p), Some(f)) = (&pending_name, &free_name) {
                    p {
                        "The free version works on one app, and yours is "
                        strong { "{f}" }
                        ". To open "
                        strong { "{p}" }
                        " too, get ShipScreen Pro, or make it your free app instead."
                    }
                } else if matches!(status, Status::Free) || status.free_credits().is_some() {
                    p {
                        "The free version works on one app"
                        if let Some(f) = &free_name { ": " strong { "{f}" } }
                        ", with every step from build to store submission. "
                        "Pro works on any number of apps, and is the licence for commercial use. "
                        "It comes with a monthly plan on mayorana.ch."
                    }
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| open_url(licence::BUY_URL),
                    if asks_number { "Buy ShipScreen Pro" } else { "Renew ShipScreen Pro" }
                }
            }
            if asks_number {
                p { class: "settings-hint", "Already subscribed? Paste the licence number from your email:" }
                input {
                    class: "licence-key",
                    placeholder: "ABCDE-FGHJK-LMNPQ-RSTUV",
                    value: "{key}",
                    oninput: move |e| key.set(e.value()),
                }
                p { class: "settings-hint",
                    "The number is checked with api0 now and once a day. Pro keeps working for a week without a connection."
                }
            }
            if offers_free {
                div { class: "free-credits",
                    p { class: "settings-hint", "No plan? Get free AI credits every month, without Pro. The licence number comes by email." }
                    div { class: "free-row",
                        input {
                            class: "licence-key",
                            r#type: "email",
                            placeholder: "you@example.com",
                            value: "{free_email}",
                            oninput: move |e| free_email.set(e.value()),
                        }
                        button {
                            class: "btn",
                            disabled: free_email.read().trim().is_empty() || busy(),
                            onclick: move |_| ask_free(),
                            "Get free AI credits"
                        }
                    }
                    if let Some(n) = free_note.read().as_ref() {
                        p { class: "pro-ok small", "{n}" }
                    }
                }
            }
            if let Some(e) = error.read().as_ref() {
                p { class: "pro-error", "{e}" }
            }
            div { class: "pro-actions",
                if asks_number {
                    button {
                        class: "btn",
                        disabled: key.read().trim().is_empty() || busy(),
                        onclick: move |_| activate(),
                        "Activate"
                    }
                }
                if let (Some(dir), Some(name)) = (pending.clone(), pending_name.clone()) {
                    button {
                        class: "btn",
                        title: "The other app stays on disk; only the free slot moves",
                        onclick: move |_| {
                            set_free_app(&dir);
                            pro.open.set(false);
                            pro.pending.set(None);
                            on_open.call(dir.clone());
                        },
                        "Make {name} my free app"
                    }
                }
                if let (Some(number), false) = (status.licence().map(str::to_owned), asks_number) {
                    button {
                        class: "btn",
                        disabled: busy(),
                        title: "Change or cancel the plan on mayorana.ch",
                        onclick: move |_| manage(number.clone()),
                        "Manage subscription"
                    }
                    button {
                        class: "btn",
                        disabled: busy(),
                        onclick: move |_| check_again(),
                        "Check again"
                    }
                }
                if status.licence().is_some() {
                    button {
                        class: "btn",
                        title: "To move the licence to another computer",
                        onclick: move |_| {
                            licence::deactivate();
                            pro.status.set(licence::current());
                        },
                        "Remove the licence from this computer"
                    }
                }
                button { class: "btn", onclick: move |_| close(), "Close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_app_takes_the_slot_and_a_second_is_locked() {
        let a = Path::new("/nowhere/app-a");
        let b = Path::new("/nowhere/app-b");
        assert_eq!(decide(&Status::Free, None, a), Decision::TakeSlot);
        assert_eq!(decide(&Status::Free, Some(a), a), Decision::Open);
        assert_eq!(decide(&Status::Free, Some(a), b), Decision::Locked);
    }

    #[test]
    fn pro_opens_anything_and_a_lapsed_plan_does_not() {
        let a = Path::new("/nowhere/app-a");
        let b = Path::new("/nowhere/app-b");
        let answer = |active| licence::Answer {
            licence: "ABCDE-FGHJK-LMNPQ-RSTUV".into(),
            products: vec!["shipscreen".into()],
            active,
            ..licence::Answer::default()
        };
        assert_eq!(
            decide(&Status::Pro(answer(true)), Some(a), b),
            Decision::Open
        );
        assert_eq!(
            decide(&Status::NotPro(answer(false)), Some(a), b),
            Decision::Locked
        );
    }
}
