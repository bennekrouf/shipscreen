//! The Screenshots step.

use super::*;

/// Checkbox list for one platform's export sizes.
pub(crate) fn target_checks(
    ws: Ws,
    prefix: &'static str,
    targets: &'static [(&'static str, &'static str, u32, u32)],
    enabled: Vec<bool>,
    set: fn(&mut ProjectState, usize, bool),
) -> Element {
    rsx! {
        div { class: "export-targets",
            for (ti, &(_, tname, tw, th)) in targets.iter().enumerate() {
                div { class: "export-target-row",
                    input {
                        r#type: "checkbox",
                        id: "chk-{prefix}-{ti}",
                        checked: enabled.get(ti).copied().unwrap_or(true),
                        onchange: move |e: Event<FormData>| ws.update(|p| set(p, ti, e.value() == "true")),
                    }
                    label { r#for: "chk-{prefix}-{ti}", class: "export-target-label",
                        span { class: "export-target-name", "{tname}" }
                        span { class: "export-target-dim", "{tw}×{th}" }
                    }
                }
            }
        }
    }
}

/// Colour swatch + hex field bound to one `ProjectState` colour.
pub(crate) fn color_field(
    ws: Ws,
    label: &'static str,
    placeholder: &'static str,
    get: fn(&ProjectState) -> &String,
    set: fn(&mut ProjectState, String),
) -> Element {
    let value = get(&ws.proj.read()).clone();
    rsx! {
        div { class: "color-field",
            label { "{label}" }
            div { class: "color-picker-row",
                input {
                    r#type: "color",
                    value: "{value}",
                    oninput: move |e: Event<FormData>| ws.update(|p| set(p, e.value())),
                }
                input {
                    r#type: "text",
                    class: "hex-input",
                    value: "{value}",
                    placeholder: "{placeholder}",
                    maxlength: 7,
                    oninput: move |e: Event<FormData>| {
                        let v = e.value();
                        let hex = if v.starts_with('#') { v } else { format!("#{v}") };
                        if hex.len() == 7 && hex[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                            ws.update(|p| set(p, hex));
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub(crate) fn ScreenshotsStep() -> Element {
    let ws = use_context::<Ws>();
    let mut gen_phase = ws.gen_phase;
    let p = ws.proj.read().clone();
    let plat = p.platform_type.clone();

    let phase = gen_phase.read().clone();
    let is_busy = matches!(
        phase,
        AppPhase::GeneratingAi | AppPhase::GeneratingManual | AppPhase::Resizing
    );
    let missing: Vec<String> = p
        .locales
        .iter()
        .filter(|l| p.sources_for(l).is_empty())
        .map(|l| locale_name(l))
        .collect();

    rsx! {
        if !missing.is_empty() {
            div { class: "card notice-card",
                p { {icon_alert()} "No source images yet for: {missing.join(\", \")}." }
                button { class: "btn btn-sm", onclick: move |_| ws.go(Step::Languages), "Go to Languages" }
            }
        }

        // ---- Sizes ----
        div { class: "card export-settings-card",
            h2 { "Sizes" }
            div { class: "export-settings-grid",
                if plat.has_desktop() {
                    div { class: "export-section",
                        div { class: "export-platform-header",
                            span { class: "export-platform-label", {icon_monitor()} "Desktop" }
                        }
                        {target_checks(ws, "desktop", DESKTOP_TARGETS, p.desktop_targets.clone(), |p, i, v| {
                            if let Some(t) = p.desktop_targets.get_mut(i) { *t = v; }
                        })}
                        p { class: "settings-hint", "Run your desktop app, take screenshots, and add them in the Languages step." }
                    }
                }

                if plat.has_ios() || p.export_ios {
                    div { class: "export-section",
                        div { class: "export-platform-header",
                            input {
                                r#type: "checkbox",
                                id: "chk-ios",
                                checked: p.export_ios,
                                onchange: move |e: Event<FormData>| ws.update(|p| p.export_ios = e.value() == "true"),
                            }
                            label { r#for: "chk-ios", class: "export-platform-label export-ios-label", "iOS" }
                        }
                        if p.export_ios {
                            {target_checks(ws, "ios", IOS_TARGETS, p.ios_targets.clone(), |p, i, v| {
                                if let Some(t) = p.ios_targets.get_mut(i) { *t = v; }
                            })}
                        }
                    }
                }

                if plat.has_android() || p.export_android {
                    div { class: "export-section",
                        div { class: "export-platform-header",
                            input {
                                r#type: "checkbox",
                                id: "chk-android",
                                checked: p.export_android,
                                onchange: move |e: Event<FormData>| ws.update(|p| p.export_android = e.value() == "true"),
                            }
                            label { r#for: "chk-android", class: "export-platform-label export-android-label", "Android" }
                        }
                        if p.export_android {
                            {target_checks(ws, "android", ANDROID_TARGETS, p.android_targets.clone(), |p, i, v| {
                                if let Some(t) = p.android_targets.get_mut(i) { *t = v; }
                            })}
                        }
                    }
                }
            }
        }

        // ---- Style ----
        div { class: "style-cards",
            div { class: "card",
                h2 { "Manual style" }
                p { class: "hint card-hint", "Captions from the Languages step on your colours. Screens alternate between the two." }
                div { class: "color-inputs",
                    {color_field(ws, "Primary", "#3B82F6", |p| &p.primary_color, |p, v| p.primary_color = v)}
                    {color_field(ws, "Secondary", "#FFFFFF", |p| &p.secondary_color, |p, v| p.secondary_color = v)}
                }
            }

            div { class: "card",
                h2 { "AI style" }
                p { class: "hint card-hint", "Describe the background; fal.ai draws a device frame and your screenshot goes inside it." }
                textarea {
                    class: "text-input theme-textarea",
                    placeholder: "e.g. dark cyberpunk neon interface with glitch effects…",
                    value: "{p.theme_prompt}",
                    rows: "3",
                    oninput: move |e: Event<FormData>| ws.update(|p| p.theme_prompt = e.value()),
                }
                if !p.theme_history.is_empty() {
                    div { class: "theme-history",
                        p { class: "theme-history-label", "Recent:" }
                        div { class: "theme-chips",
                            for entry in p.theme_history.iter().cloned() {
                                button {
                                    class: "theme-chip",
                                    title: "{entry}",
                                    onclick: {
                                        let entry = entry.clone();
                                        move |_| ws.update(|p| p.theme_prompt = entry.clone())
                                    },
                                    if entry.chars().count() > 60 {
                                        "{entry.chars().take(57).collect::<String>()}…"
                                    } else {
                                        "{entry}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ---- Generate ----
        div { class: "generate-row",
            button {
                class: "btn btn-generate btn-manual",
                disabled: is_busy,
                onclick: move |_| jobs::generate_manual(ws),
                if matches!(phase, AppPhase::GeneratingManual) {
                    span { class: "spinner spinner-dark" }
                    " Generating…"
                } else {
                    {icon_image()}
                    "Generate Manual"
                }
            }
            button {
                class: "btn btn-primary btn-generate",
                disabled: is_busy,
                onclick: move |_| jobs::generate_ai(ws),
                if matches!(phase, AppPhase::GeneratingAi | AppPhase::Resizing) {
                    span { class: "spinner" }
                    " Generating AI…"
                } else {
                    {icon_sparkle()}
                    "Generate with AI"
                }
            }
        }

        if let AppPhase::Error(msg) = &phase {
            div { class: "card error-card", role: "alert",
                p { {icon_alert()} "{msg}" }
                button { class: "btn btn-dismiss", onclick: move |_| gen_phase.set(AppPhase::Idle), "Dismiss" }
            }
        }

        // ---- Results ----
        div { class: "card",
            h2 { "Results" }
            if p.generated_urls.is_empty() {
                p { class: "output-empty", "Generated screenshots will appear here." }
            } else {
                div { class: "preview-grid",
                    for (i, url) in p.generated_urls.iter().cloned().enumerate() {
                        div { class: "preview-item",
                            p { class: "preview-label", "Screen {i + 1}" }
                            img { class: "preview-img", src: "{url}", alt: "Generated screenshot {i + 1}" }
                        }
                    }
                }
            }
            if !p.output_paths.is_empty() {
                details { class: "saved-files",
                    summary { "{p.output_paths.len()} saved files" }
                    for (label, path) in p.output_paths.iter().cloned() {
                        div { class: "output-row",
                            span { class: "output-label", "{label}" }
                            span { class: "output-path", "{path.display()}" }
                        }
                    }
                }
            }
        }
    }
}
