//! The window's root: the project picker until a project is open, then its
//! workspace.

use super::*;
use desktop_kit::ui::{use_pro, Banners, Header, ProButton, ProWindow, Theme};

#[component]
pub(super) fn App() -> Element {
    // Global settings
    let mut settings = use_signal(load_settings);
    use_context_provider(|| settings);

    // ShipScreen Pro: the free version works on one app (see pro.rs).
    // The licence is checked with api0 at start, then once a day.
    let mut pro = use_pro();
    // The app someone tried to open while the free slot holds another one.
    let mut pending = use_signal(|| Option::<PathBuf>::None);
    // The title bar follows the licence, however it changes.
    use_effect(move || {
        dioxus::desktop::window().set_title(&window_title(&pro.status.read()));
    });

    // Active project dir (None = show project picker)
    let mut project_dir = use_signal(|| Option::<PathBuf>::None);
    // Opens a project folder and records it in recents. Every way in goes
    // through here, after the Pro check.
    let open_project = use_callback(move |dir: PathBuf| {
        let mut s = settings.write();
        s.recent_projects.retain(|p| p != &dir);
        s.recent_projects.insert(0, dir.clone());
        s.recent_projects.truncate(MAX_RECENT_PROJECTS);
        save_settings(&s);
        drop(s);
        project_dir.set(Some(dir));
    });

    // ── Theme (matches ais-runner) ────────────────────────────────────────
    // Initialise from system, then keep in sync — but stop syncing once the
    // user manually toggles via the button.
    let mut is_light = use_signal(system_prefers_light);
    let theme_overridden = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let mut show_mcp = use_signal(|| false);

    // Lock body scroll whenever the settings popup is open
    use_effect(move || {
        let locked = *show_settings.read();
        let js = if locked {
            "document.body.style.overflow = 'hidden';"
        } else {
            "document.body.style.overflow = '';"
        };
        spawn(async move {
            document::eval(js).await.ok();
        });
    });

    use_effect(move || {
        let cls = if *is_light.read() { "light" } else { "" };
        document::eval(&format!("document.body.className = '{}';", cls));
    });

    use_coroutine(
        move |_rx: dioxus::prelude::UnboundedReceiver<()>| async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                if *theme_overridden.read() {
                    continue;
                }
                let light = tokio::task::spawn_blocking(system_prefers_light)
                    .await
                    .unwrap_or(*is_light.read());
                if light != *is_light.read() {
                    is_light.set(light);
                }
            }
        },
    );

    // Asset handler for local images (thumbnails)
    use_asset_handler("localimg", |request, responder: RequestAsyncResponder| {
        let encoded = request
            .uri()
            .path()
            .strip_prefix("/localimg/")
            .unwrap_or("")
            .to_string();
        let decoded = urlencoding::decode(&encoded)
            .unwrap_or_default()
            .into_owned();
        let file_path = PathBuf::from(decoded);
        tokio::spawn(async move {
            match tokio::fs::read(&file_path).await {
                Ok(bytes) => {
                    let mime = match file_path.extension().and_then(|e| e.to_str()) {
                        Some("png") => "image/png",
                        Some("jpg") | Some("jpeg") => "image/jpeg",
                        Some("webp") => "image/webp",
                        _ => "image/png",
                    };
                    responder.respond(
                        Response::builder()
                            .status(StatusCode::OK)
                            .header("Content-Type", mime)
                            .header("Access-Control-Allow-Origin", "*")
                            .body(bytes)
                            .unwrap_or_default(),
                    );
                }
                Err(_) => {
                    responder.respond(
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(vec![])
                            .unwrap_or_default(),
                    );
                }
            }
        });
    });

    rsx! {
        Header {
            theme: Theme { light: is_light, picked: theme_overridden },
            on_settings: move |_| show_settings.toggle(),
            on_mcp: move |_| show_mcp.toggle(),
            pro: rsx! { ProButton { pro, hint: pro::HINT } },
        }
        if show_settings() {
            SettingsPopup { on_close: move |_| show_settings.set(false) }
        }
        if show_mcp() {
            workspace::mcp::McpPopup { on_close: move |_| show_mcp.set(false) }
        }

        Banners { statistics: true }

        ProWindow {
            pro,
            pitch: pro::pitch(pending.read().as_deref()),
            actions: pro::make_free(pro, pending, open_project),
            // The app that was waiting opens behind the window, which stays up
            // to say Pro is active.
            on_pro: move |_| {
                if let Some(dir) = pending.take() {
                    open_project.call(dir);
                }
            },
            on_close: move |_| pending.set(None),
        }

        if let Some(dir) = project_dir.read().clone() {
            ProjectView {
                project_dir: dir,
                on_close: move |_| project_dir.set(None),
            }
        } else {
            ProjectPicker {
                on_open: move |dir: PathBuf| {
                    // Without Pro, a second app asks for Pro first.
                    if pro::may_open(&pro.status.read(), &dir) {
                        open_project.call(dir);
                    } else {
                        pending.set(Some(dir));
                        pro.open.set(true);
                    }
                }
            }
        }
    }
}
