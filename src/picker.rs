//! The first screen: picking or creating a project, and the settings.

use super::*;

#[cfg(not(target_os = "android"))]
pub(super) fn open_existing_button(on_open: EventHandler<PathBuf>) -> Element {
    rsx! {
        button {
            class: "btn picker-open-btn",
            onclick: move |_| {
                spawn(async move {
                    if let Some(folder) = rfd::AsyncFileDialog::new()
                        .set_title("Open existing project folder")
                        .pick_folder()
                        .await
                    {
                        on_open.call(folder.path().to_path_buf());
                    }
                });
            },
            "📂  Open Existing…"
        }
    }
}

#[cfg(target_os = "android")]
pub(super) fn open_existing_button(_on_open: EventHandler<PathBuf>) -> Element {
    rsx! {}
}

#[cfg(not(target_os = "android"))]
pub(super) fn parent_folder_field(mut new_parent: Signal<Option<PathBuf>>) -> Element {
    rsx! {
        div { class: "build-config-field",
            label { class: "build-config-label", "Location" }
            div { class: "folder-pick-row",
                button {
                    class: "btn",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(folder) = rfd::AsyncFileDialog::new()
                                .set_title("Choose parent folder for new project")
                                .pick_folder()
                                .await
                            {
                                new_parent.set(Some(folder.path().to_path_buf()));
                            }
                        });
                    },
                    "Choose Folder…"
                }
                if let Some(p) = new_parent.read().clone() {
                    span { class: "folder-pick-path", "{p.to_string_lossy()}" }
                } else {
                    span { class: "folder-pick-hint", "No folder selected" }
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
pub(super) fn parent_folder_field(_new_parent: Signal<Option<PathBuf>>) -> Element {
    rsx! {}
}

#[component]
pub(super) fn ProjectPicker(on_open: EventHandler<PathBuf>) -> Element {
    let settings = use_context::<Signal<Settings>>();

    // "New Project" wizard state
    let mut show_new = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_slug = use_signal(String::new);
    let mut new_bundle = use_signal(String::new);
    let mut new_platform = use_signal(|| PlatformType::IosAndroid);
    // Desktop: nothing until the user picks a folder. Android: there's no
    // per-project folder choice at all (see src/android_saf.rs) — every
    // project lives under one fixed, app-private root, so this is never
    // empty and the "Location" field below doesn't render.
    #[cfg(not(target_os = "android"))]
    let new_parent = use_signal(|| Option::<PathBuf>::None);
    #[cfg(target_os = "android")]
    let new_parent = use_signal(|| Some(android_saf::app_private_projects_root()));
    let mut create_error = use_signal(|| Option::<String>::None);

    // Derive slug and bundle from name automatically (user can override)
    let auto_slug = {
        let n = new_name.read().to_lowercase();
        n.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
    };
    let auto_bundle = {
        let s = if new_slug.read().is_empty() {
            auto_slug.clone()
        } else {
            new_slug.read().clone()
        };
        if s.is_empty() {
            "com.company.app".to_string()
        } else {
            format!("com.mayorana.{s}")
        }
    };

    rsx! {
        div { class: "picker-screen",
            div { class: "picker-inner",
                div { class: "picker-logo",
                    img {
                        class: "picker-logo-mark",
                        src: logo_mark_src(),
                        alt: "",
                    }
                    div {
                        h1 { "ShipScreen" }
                        p { class: "picker-subtitle", "App Store & Play Store screenshot generator" }
                    }
                }

                // ── Action buttons row ──────────────────────────────────────
                div { class: "picker-actions",
                    button {
                        // Only "New Project…" is primary. When this toggles to
                        // "Cancel" it demotes to a plain button — the strongest
                        // blue on the first screen should never be the dismiss.
                        class: if *show_new.read() { "btn picker-new-btn" } else { "btn btn-primary picker-new-btn" },
                        onclick: move |_| {
                            let currently = *show_new.read();
                            show_new.set(!currently);
                            create_error.set(None);
                        },
                        if *show_new.read() {
                            {icon_close()}
                            "Cancel"
                        } else {
                            {icon_sparkle()}
                            "New Project…"
                        }
                    }
                    {open_existing_button(on_open)}
                }

                // ── New Project wizard (inline) ──────────────────────────────
                if *show_new.read() {
                    div { class: "new-project-panel",
                        h3 { class: "new-project-title", "Create New Project" }

                        // App Name
                        div { class: "build-config-field",
                            label { class: "build-config-label", "App Name" }
                            input {
                                class: "text-input",
                                placeholder: "My App",
                                value: "{new_name.read()}",
                                oninput: move |e: Event<FormData>| {
                                    let v = e.value();
                                    // Auto-derive slug when empty
                                    if new_slug.read().is_empty() {
                                        // leave blank so auto_slug kicks in
                                    }
                                    new_name.set(v);
                                }
                            }
                        }

                        // Slug (auto-derived, editable)
                        div { class: "build-config-field",
                            label { class: "build-config-label", "Project Slug" }
                            input {
                                class: "text-input",
                                placeholder: "{auto_slug}",
                                value: "{new_slug.read()}",
                                oninput: move |e: Event<FormData>| new_slug.set(e.value())
                            }
                            p { class: "settings-hint", "Lowercase identifier — leave blank to auto-derive from App Name" }
                        }

                        // Bundle ID
                        div { class: "build-config-field",
                            label { class: "build-config-label", "Bundle ID" }
                            input {
                                class: "text-input",
                                placeholder: "{auto_bundle}",
                                value: "{new_bundle.read()}",
                                oninput: move |e: Event<FormData>| new_bundle.set(e.value())
                            }
                            p { class: "settings-hint", "Leave blank for com.mayorana.&lt;slug&gt;" }
                        }

                        // Platform selector
                        div { class: "build-config-field",
                            label { class: "build-config-label", "Platform" }
                            div { class: "platform-selector",
                                for plat in [PlatformType::IosAndroid, PlatformType::Ios, PlatformType::Android, PlatformType::Desktop] {
                                    {
                                        let label = plat.label();
                                        let is_selected = *new_platform.read() == plat;
                                        let plat2 = plat.clone();
                                        rsx! {
                                            button {
                                                class: if is_selected { "platform-btn platform-btn-active" } else { "platform-btn" },
                                                onclick: move |_| new_platform.set(plat2.clone()),
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        {parent_folder_field(new_parent)}

                        // Error banner
                        if let Some(err) = create_error.read().clone() {
                            p { class: "new-project-error", "{err}" }
                        }

                        // Create button
                        button {
                            class: "btn btn-primary",
                            onclick: {
                                let on_open2 = on_open;
                                move |_| {
                                    let name_val  = new_name.read().trim().to_string();
                                    let slug_raw  = new_slug.read().trim().to_string();
                                    let slug_val  = if slug_raw.is_empty() { auto_slug.clone() } else { slug_raw };
                                    let bundle_raw = new_bundle.read().trim().to_string();
                                    let bundle_val = if bundle_raw.is_empty() { auto_bundle.clone() } else { bundle_raw };
                                    let plat_val  = new_platform.read().clone();
                                    let parent    = new_parent.read().clone();

                                    if name_val.is_empty() {
                                        create_error.set(Some("App Name is required.".into()));
                                        return;
                                    }
                                    if slug_val.is_empty() {
                                        create_error.set(Some("Could not derive a slug — please fill in Project Slug.".into()));
                                        return;
                                    }
                                    let Some(parent_dir) = parent else {
                                        create_error.set(Some("Please choose a parent folder.".into()));
                                        return;
                                    };

                                    let project_dir = parent_dir.join(&slug_val);
                                    match scaffold_new_project(&project_dir, &name_val, &slug_val, &bundle_val, &plat_val) {
                                        Ok(()) => {
                                            // Pre-populate shipscreen.json with the config so
                                            // ProjectView opens with all fields filled in.
                                            let mut state = ProjectState::with_defaults();
                                            state.app_name          = name_val;
                                            state.project_slug      = slug_val;
                                            state.ios_bundle_id     = bundle_val.clone();
                                            state.android_bundle_id = bundle_val;
                                            state.platform_type = plat_val;
                                            // Disable iOS/Android targets for desktop-only projects
                                            if state.platform_type.has_desktop() && !state.platform_type.has_ios() {
                                                state.export_ios     = false;
                                                state.export_android = false;
                                            }
                                            save_project_state(&project_dir, &state);
                                            show_new.set(false);
                                            create_error.set(None);
                                            on_open2.call(project_dir);
                                        }
                                        Err(e) => create_error.set(Some(e)),
                                    }
                                }
                            },
                            {icon_folder_plus()}
                            "Create Project"
                        }
                    }
                }

                // ── Recent Projects ──────────────────────────────────────────
                if !settings.read().recent_projects.is_empty() {
                    div { class: "picker-recents",
                        p { class: "picker-recents-label", "Recent Projects" }
                        for proj in settings.read().recent_projects.clone().iter() {
                            {
                                let proj = proj.clone();
                                let proj2 = proj.clone();
                                let name = proj.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                let path_str = proj.to_string_lossy().to_string();
                                rsx! {
                                    button {
                                        class: "picker-recent-item",
                                        onclick: move |_| on_open.call(proj2.clone()),
                                        div { class: "picker-recent-name", "{name}" }
                                        div { class: "picker-recent-path", "{path_str}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn SettingsPopup(on_close: EventHandler<()>) -> Element {
    let mut settings = use_context::<Signal<Settings>>();

    let fal_key_val = settings.read().fal_key.clone();
    let phone_style_val = settings.read().phone_style.clone();
    let inference_steps_val = settings.read().inference_steps;

    rsx! {
        div { class: "settings-backdrop", onclick: move |_| on_close.call(()) }
        div {
            class: "settings-popup settings-popup-wide",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Settings",
            // Esc closes it. Backdrop-click was the only way out before, which
            // was the sole escape whenever the window was short enough to push
            // the close button off-screen.
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
            div { class: "modal-head",
                h2 { "Settings" }
                button { class: "modal-close", title: "Close", onclick: move |_| on_close.call(()), "×" }
            }
            div { class: "modal-body",

                // ---- Screenshot Generation ----
                p { class: "settings-section-title", "Screenshot Generation" }

                div { class: "settings-field",
                    label { "fal.ai API Key" }
                    input {
                        class: "text-input",
                        r#type: "password",
                        placeholder: "Enter your fal.ai API key",
                        value: "{fal_key_val}",
                        oninput: move |e: Event<FormData>| { settings.write().fal_key = e.value(); save_settings(&settings()); },
                    }
                    p { class: "settings-hint", "Get a key at fal.ai/dashboard/keys" }
                }

                div { class: "settings-field",
                    label { "Device Style" }
                    input {
                        class: "text-input",
                        placeholder: "modern smartphone",
                        value: "{phone_style_val}",
                        oninput: move |e: Event<FormData>| { settings.write().phone_style = e.value(); save_settings(&settings()); },
                    }
                    p { class: "settings-hint", "e.g. modern smartphone, iPhone 16, iPad Pro" }
                }

                div { class: "settings-field",
                    label { "Inference Steps" }
                    input {
                        class: "text-input",
                        r#type: "number",
                        min: "10",
                        max: "50",
                        value: "{inference_steps_val}",
                        oninput: move |e: Event<FormData>| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                settings.write().inference_steps = v.clamp(10, 50);
                                save_settings(&settings());
                            }
                        },
                    }
                    p { class: "settings-hint", "More steps = better quality but slower (10–50)" }
                }

                div { class: "settings-field settings-path",
                    p { class: "settings-hint", "Config: {global_config_path().display()}" }
                }
            }
        }
    }
}
