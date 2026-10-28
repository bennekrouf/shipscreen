//! ShipScreen Pro: the free version's one app, and what the Pro window says
//! about it. The window, the badge and the licence checks are desktop-kit's.
//!
//! Without Pro, ShipScreen works on one app (project folder): the first one
//! opened takes the free slot, kept in `free_app.json` beside the licence.
//! Opening another asks for Pro, or offers to make that one the free app
//! instead — the same deal as GitAgent's free repositories.

use std::path::{Path, PathBuf};

use desktop_kit::ui::Pro;
use dioxus::prelude::*;

use crate::licence::{self, Status};

const FREE_APP_FILE: &str = "free_app.json";

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
    if status.is_pro() {
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

/// The badge's tooltip while ShipScreen isn't Pro.
pub const HINT: &str = "The free version works on one app; Pro on any number";

/// What the free version does and what Pro adds, in the Pro window: about
/// `pending`, when it was opened for an app the free slot doesn't hold.
pub fn pitch(pending: Option<&Path>) -> Element {
    let free_name = free_app().as_deref().map(folder_name);
    let pending_name = pending.map(folder_name);
    rsx! {
        if let (Some(p), Some(f)) = (&pending_name, &free_name) {
            p {
                "The free version works on one app, and yours is "
                strong { "{f}" }
                ". To open "
                strong { "{p}" }
                " too, get ShipScreen Pro, or make it your free app instead."
            }
        } else {
            p {
                "The free version works on one app"
                if let Some(f) = &free_name { ": " strong { "{f}" } }
                ", with every step from build to store submission. "
                "Pro works on any number of apps, and is the licence for commercial use. "
                "It comes with a monthly plan on mayorana.ch."
            }
        }
    }
}

/// "Make it my free app", while an app waits for the free slot: moves the
/// slot to it, closes the window and opens it with `on_open`.
pub fn make_free(
    pro: Pro,
    mut pending: Signal<Option<PathBuf>>,
    on_open: EventHandler<PathBuf>,
) -> Option<Element> {
    let dir = pending.read().clone()?;
    let name = folder_name(&dir);
    let mut open = pro.open;
    Some(rsx! {
        button {
            class: "kit-pro-btn",
            title: "The other app stays on disk; only the free slot moves",
            onclick: move |_| {
                set_free_app(&dir);
                open.set(false);
                pending.set(None);
                on_open.call(dir.clone());
            },
            "Make {name} my free app"
        }
    })
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
        let answer = |active| desktop_kit::licence::Answer {
            licence: "ABCDE-FGHJK-LMNPQ-RSTUV".into(),
            products: vec!["shipscreen".into()],
            active,
            ..desktop_kit::licence::Answer::default()
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
