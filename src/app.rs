//! The window's root: the project picker until a project is open, then its
//! workspace.

use super::*;
use desktop_kit::ui::{Header, Theme};

#[component]
pub(super) fn App() -> Element {
    // Global settings
    let mut settings = use_signal(load_settings);
    use_context_provider(|| settings);

    // ShipScreen Pro: the free version works on one app (see pro.rs).
    let mut pro = use_context_provider(|| pro::Pro {
        open: Signal::new(false),
        status: Signal::new(licence::current()),
        pending: Signal::new(None),
    });
    // The title bar follows the licence, however it changes.
    use_effect(move || {
        dioxus::desktop::window().set_title(&window_title(&pro.status.read()));
    });
    // The licence is checked with api0 at start, then once a day.
    use_future(move || async move {
        pro.status.set(licence::refresh(true).await);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            pro.status.set(licence::refresh(false).await);
        }
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

    // ── Auto-update check ──────────────────────────────────────────────────
    // Background fetch latest.json from GitHub releases on startup; if a newer
    // version is published, surface a small banner. Dismissable per session.
    // ── Notice from mayorana.ch ────────────────────────────────────────────
    // A message to the people running this build (see notice.rs). Same
    // posture as the update check: delayed, best-effort, silent on failure.
    let mut mayorana_notice = use_signal(|| Option::<notice::Notice>::None);
    use_coroutine(
        move |_rx: dioxus::prelude::UnboundedReceiver<()>| async move {
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            if let Some(n) = notice::fetch().await {
                mayorana_notice.set(Some(n));
            }
        },
    );

    let mut update_info = use_signal(|| Option::<update_check::UpdateInfo>::None);
    let mut update_dismissed = use_signal(|| false);

    use_coroutine(
        move |_rx: dioxus::prelude::UnboundedReceiver<()>| async move {
            // Small delay so we don't compete with the app's own boot work.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Some(info) = update_check::check().await {
                telemetry::record(telemetry::Event::UpdateOffered {
                    to: info.latest_version.clone(),
                });
                update_info.set(Some(info));
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

    // ── Anonymous usage statistics ─────────────────────────────────────────
    // On by default, but only once the person has been told: `start` reads the
    // opt-outs (including a shell profile's), records the launch, and says
    // whether the notice is still owed. See telemetry.rs.
    let mut ask_consent = use_signal(|| false);
    use_coroutine(
        move |_rx: dioxus::prelude::UnboundedReceiver<()>| async move {
            if telemetry::start().await {
                ask_consent.set(true);
            }
            telemetry::flush_forever().await;
        },
    );
    // Recorded while the notice is on screen, so collection starts from the
    // next event — never from one the person had no chance to read about.
    use_effect(move || {
        if *ask_consent.read() {
            telemetry::mark_informed();
        }
    });

    rsx! {
        Header {
            theme: Theme { light: is_light, picked: theme_overridden },
            on_settings: move |_| show_settings.toggle(),
            on_mcp: move |_| show_mcp.toggle(),
            pro: rsx! { pro::ProButton {} },
        }
        if show_settings() {
            SettingsPopup { on_close: move |_| show_settings.set(false) }
        }
        if show_mcp() {
            workspace::mcp::McpPopup { on_close: move |_| show_mcp.set(false) }
        }

        // ── Update banner ──────────────────────────────────────────────────
        // Renders only when a newer release exists AND the user hasn't dismissed
        // it this session. "Download" opens this OS's build in the system
        // browser, which downloads it (Dioxus opens external links via the OS
        // handler).
        if let (Some(info), false) = (update_info.read().clone(), *update_dismissed.read()) {
            div { class: "update-banner",
                span { class: "update-banner-text",
                    "ShipScreen "
                    strong { "{info.latest_version}" }
                    " is available (you have {env!(\"CARGO_PKG_VERSION\")})."
                }
                a {
                    class: "update-banner-link",
                    href: "{info.download_url}",
                    target: "_blank",
                    onclick: {
                        let to = info.latest_version.clone();
                        move |_| telemetry::record(telemetry::Event::UpdateClicked { to: to.clone() })
                    },
                    "Download"
                }
                button {
                    class: "update-banner-dismiss",
                    title: "Dismiss",
                    aria_label: "Dismiss update notice",
                    onclick: move |_| update_dismissed.set(true),
                    "×"
                }
            }
        }

        // Usage-statistics notice: once, at the bottom so it never sits under the
        // update or notice banners. Either button is remembered.
        if *ask_consent.read() {
            div { class: "consent-banner",
                span { class: "update-banner-text",
                    strong { "ShipScreen shares anonymous usage statistics. " }
                    "Whether it is installed and opened, and its version and operating system \
                     — never your files, data, accounts or anything you type."
                }
                button {
                    class: "update-banner-link",
                    onclick: move |_| {
                        telemetry::set_consent(true);
                        ask_consent.set(false);
                    },
                    "OK"
                }
                button {
                    class: "update-banner-link",
                    onclick: move |_| {
                        telemetry::set_consent(false);
                        ask_consent.set(false);
                    },
                    "Turn off"
                }
            }
        }

        // Notice from mayorana.ch — shown until dismissed, then remembered.
        if let Some(n) = mayorana_notice.read().clone() {
            {
                let id = n.id.clone();
                let link_text = n.link_text.clone().unwrap_or_else(|| "Open".to_string());
                rsx! {
                    div { class: "update-banner notice-banner",
                        span { class: "update-banner-text", "{n.text}" }
                        if let Some(url) = n.url.clone() {
                            a {
                                class: "update-banner-link",
                                href: "{url}",
                                target: "_blank",
                                "{link_text}"
                            }
                        }
                        button {
                            class: "update-banner-dismiss",
                            onclick: move |_| {
                                notice::dismiss(&id);
                                mayorana_notice.set(None);
                            },
                            "×"
                        }
                    }
                }
            }
        }

        pro::ProWindow { on_open: open_project }

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
                        pro.pending.set(Some(dir));
                        pro.open.set(true);
                    }
                }
            }
        }
    }
}
