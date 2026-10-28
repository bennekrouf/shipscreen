//! The store listing and Submit steps.

use super::*;

/// A store-text field for the active language, with the store's length limit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn store_text_field(
    ws: Ws,
    locale: String,
    label: &'static str,
    hint: &'static str,
    limit: usize,
    rows: u32,
    get: fn(&StoreText) -> &String,
    set: fn(&mut StoreText, String),
) -> Element {
    let value = ws
        .proj
        .read()
        .store_texts
        .get(&locale)
        .map(|t| get(t).clone())
        .unwrap_or_default();
    let used = value.chars().count();
    rsx! {
        div { class: "settings-field",
            div { class: "field-head",
                label { "{label}" }
                span { class: if used > limit { "char-count char-over" } else { "char-count" }, "{used}/{limit}" }
            }
            if rows > 1 {
                textarea {
                    class: "text-input",
                    rows: "{rows}",
                    value: "{value}",
                    oninput: move |e: Event<FormData>| {
                        let v = e.value();
                        ws.update(|p| set(p.store_texts.entry(locale.clone()).or_default(), v));
                    },
                }
            } else {
                input {
                    class: "text-input",
                    value: "{value}",
                    oninput: move |e: Event<FormData>| {
                        let v = e.value();
                        ws.update(|p| set(p.store_texts.entry(locale.clone()).or_default(), v));
                    },
                }
            }
            if !hint.is_empty() {
                p { class: "settings-hint", "{hint}" }
            }
        }
    }
}

#[component]
pub(crate) fn MetadataStep() -> Element {
    let ws = use_context::<Ws>();
    let mut active_locale = ws.active_locale;
    let p = ws.proj.read().clone();
    let plat = p.platform_type.clone();
    let active = {
        let a = active_locale.read().clone();
        if p.locales.contains(&a) {
            a
        } else {
            p.locales.first().cloned().unwrap_or_else(|| "en-US".into())
        }
    };
    let complete = |loc: &String| {
        p.store_texts.get(loc).is_some_and(|t| {
            !t.description.trim().is_empty()
                && (!plat.has_android() || !t.short_description.trim().is_empty())
        })
    };
    let first = p.locales.first().cloned().unwrap_or_default();
    let can_copy = active != first
        && p.store_texts
            .get(&first)
            .is_some_and(|t| *t != StoreText::default());
    let title_too_long = p.app_name.chars().count() > 30;

    rsx! {
        div { class: "card card-screenshots",
            div { class: "lang-tab-bar",
                for loc in p.locales.iter().cloned() {
                    div {
                        class: if loc == active { "lang-tab lang-tab-active" } else { "lang-tab" },
                        onclick: {
                            let loc = loc.clone();
                            move |_| active_locale.set(loc.clone())
                        },
                        span { class: "lang-tab-name", "{locale_name(&loc)}" }
                        if complete(&loc) {
                            span { class: "tab-done", {icon_check()} }
                        }
                    }
                }
                div { class: "lang-tab-bar-right",
                    if can_copy {
                        button {
                            class: "btn btn-sm",
                            title: "Start from the {locale_name(&first)} text, then translate",
                            onclick: {
                                let (first, active) = (first.clone(), active.clone());
                                move |_| ws.update(|p| {
                                    if let Some(src) = p.store_texts.get(&first).cloned() {
                                        p.store_texts.insert(active.clone(), src);
                                    }
                                })
                            },
                            "Copy from {locale_name(&first)}"
                        }
                    }
                }
            }

            div { class: "meta-body",
                {store_text_field(ws, active.clone(), "Description", "Shown on the product page in both stores.", 4000, 8,
                    |t| &t.description, |t, v| t.description = v)}
                {store_text_field(ws, active.clone(), "What's new",
                    if plat.has_android() { "App Store release notes, and Play release notes (Play keeps the first 500 characters)." } else { "Release notes for this version." },
                    4000, 4, |t| &t.whats_new, |t, v| t.whats_new = v)}
            }
        }

        if plat.has_ios() {
            div { class: "card",
                h2 { "App Store" }
                {store_text_field(ws, active.clone(), "Keywords", "Comma-separated, no spaces needed. Not shown to users.", 100, 1,
                    |t| &t.keywords, |t, v| t.keywords = v)}
                {store_text_field(ws, active.clone(), "Promotional text", "Can be changed any time without a new version.", 170, 2,
                    |t| &t.promo_text, |t, v| t.promo_text = v)}
                div { class: "settings-field",
                    label { "Support URL" }
                    input {
                        class: "text-input",
                        placeholder: "https://example.com/support",
                        value: "{p.support_url}",
                        oninput: move |e: Event<FormData>| ws.update(|p| p.support_url = e.value()),
                    }
                    p { class: "settings-hint", "Same for every language. Required before App Review." }
                }
            }
        }

        if plat.has_android() {
            div { class: "card",
                h2 { "Google Play" }
                {store_text_field(ws, active.clone(), "Short description", "The one-liner under the app name.", 80, 1,
                    |t| &t.short_description, |t, v| t.short_description = v)}
                p { class: if title_too_long { "settings-hint hint-error" } else { "settings-hint" },
                    "The Play title is the App Name from the App step (\"{p.app_name}\", {p.app_name.chars().count()}/30)."
                }
            }
        }
    }
}

/// What the stores show right now, fetched on demand.
#[component]
pub(crate) fn StoreStatusCard() -> Element {
    let ws = use_context::<Ws>();
    let lines = use_signal(Vec::<(String, String)>::new);
    let state = use_signal(|| JobState::Idle);
    let plat = ws.proj.read().platform_type.clone();
    rsx! {
        div { class: "card",
            h2 { "In the stores now" }
            if !lines.read().is_empty() {
                ul { class: "check-list",
                    for (store, text) in lines.read().iter().cloned() {
                        li { class: "check-row",
                            div { class: "check-text",
                                span { class: "check-label", "{store}" }
                                span { class: "check-detail", "{text}" }
                            }
                        }
                    }
                }
            }
            div { class: "conn-test",
                button {
                    class: "btn btn-sm",
                    disabled: matches!(state(), JobState::Running(_)),
                    onclick: move |_| jobs::fetch_store_status(ws, lines, state),
                    "Refresh from the stores"
                }
                div { class: "publish-status-row", {job_status_line(state(), "Version states from App Store Connect, tracks from Google Play")} }
            }
            p { class: "settings-hint",
                "Not available through the APIs — check in the consoles: "
                if plat.has_android() {
                    a { href: "https://play.google.com/console", target: "_blank", "Play Console" }
                    " (upload key reset, policy inbox, developer verification)"
                }
                if plat.has_android() && plat.has_ios() { " · " }
                if plat.has_ios() {
                    a { href: "https://appstoreconnect.apple.com", target: "_blank", "App Store Connect" }
                    " (review messages)"
                }
            }
        }
    }
}

pub(crate) fn job_button(
    label: &str,
    running_label: &str,
    running: bool,
    disabled: bool,
    class: &str,
    onclick: impl FnMut(MouseEvent) + 'static,
) -> Element {
    rsx! {
        button {
            class: "{class}",
            disabled: running || disabled,
            onclick: onclick,
            if running {
                span { class: "spinner" }
                " {running_label}"
            } else {
                {icon_upload()}
                "{label}"
            }
        }
    }
}

#[component]
pub(crate) fn SubmitStep() -> Element {
    let ws = use_context::<Ws>();
    let p = ws.proj.read().clone();
    let plat = p.platform_type.clone();
    let creds = ws.creds.read().clone();
    let artifacts = ws.artifacts.read().clone();
    let version = p.version.trim().to_string();

    let existing = |pred: fn(&str) -> bool| -> Vec<(String, PathBuf)> {
        p.output_paths
            .iter()
            .filter(|(l, path)| pred(l) && path.exists())
            .cloned()
            .collect()
    };
    let ios_shots = existing(|l| l.starts_with("iOS "));
    let ios_langs = p
        .locales
        .iter()
        .filter(|loc| {
            ios_shots
                .iter()
                .any(|(l, _)| l.contains(&format!("[{loc}]")))
        })
        .count();
    let android_shots = existing(|l| l.contains("Android Phone") || l.contains("Android Feature"));
    let described = |need_short: bool| {
        p.locales
            .iter()
            .filter(|l| {
                p.store_texts.get(*l).is_some_and(|t| {
                    !t.description.trim().is_empty()
                        && (!need_short || !t.short_description.trim().is_empty())
                })
            })
            .count()
    };
    let running = |k: JobKind| matches!(ws.job_state(k), JobState::Running(_));
    let is_mac = cfg!(target_os = "macos");

    rsx! {
        div { class: "platform-cards",
            if plat.has_ios() {
                div { class: "card platform-card",
                    h2 { class: "build-ios-title", {icon_phone()} "App Store" }
                    ul { class: "check-list",
                        {check_row(ws, !p.ios_bundle_id.trim().is_empty() && creds.app_store_ok(), "Account",
                            if creds.app_store_ok() { p.ios_bundle_id.clone() } else { "API key missing or incomplete".into() }, Some(Step::Accounts))}
                        {check_row(ws, valid_version(&version), "Version",
                            if valid_version(&version) { version.clone() } else { "Not a valid version".into() }, Some(Step::Version))}
                        {check_row(ws, described(false) == p.locales.len() && !p.support_url.trim().is_empty(), "Store text",
                            format!("{} of {} languages described{}", described(false), p.locales.len(),
                                if p.support_url.trim().is_empty() { " · no support URL" } else { "" }), Some(Step::Metadata))}
                        {check_row(ws, !ios_shots.is_empty(), "Screenshots",
                            format!("{} files · {ios_langs} of {} languages", ios_shots.len(), p.locales.len()), Some(Step::Screenshots))}
                        {check_row(ws, artifacts.ipa.is_some(), "IPA",
                            artifacts.ipa.as_ref().map(|a| a.summary()).unwrap_or_else(|| "Not built yet".into()), Some(Step::Build))}
                        if artifacts.ipa.is_some() {
                            {build_check_row(ws, ws.ipa_check.read().clone())}
                        }
                    }

                    {release_stage("1 · Listing",
                        "Store text and screenshots for every language. Languages missing in App Store Connect are added.".into(),
                        ws.job_state(JobKind::AppStore), "Ready",
                        job_button("Upload listing", "Uploading…", running(JobKind::AppStore), false, "btn stage-btn",
                            move |_| jobs::publish_app_store(ws)))}

                    {release_stage("2 · Build",
                        if is_mac { "Uploads the newest IPA with xcrun altool, using the API key.".to_string() }
                        else { "IPA upload needs macOS (xcrun altool).".to_string() },
                        ws.job_state(JobKind::AppStoreBuild), "Ready",
                        job_button("Upload IPA", "Uploading IPA…", running(JobKind::AppStoreBuild), !is_mac || artifacts.ipa.is_none(), "btn stage-btn",
                            move |_| jobs::upload_ipa(ws)))}

                    {release_stage("3 · App Review",
                        "Waits for the uploaded build to finish processing, attaches it to the version and submits it.".into(),
                        ws.job_state(JobKind::AppStoreReview), "Ready",
                        rsx! {
                            label { class: "toggle-row",
                                input { r#type: "checkbox", checked: p.exempt_encryption, onchange: move |e: Event<FormData>| ws.update(|p| p.exempt_encryption = e.value() == "true") }
                                span { "The app uses only exempt encryption (HTTPS, OS crypto)" }
                            }
                            {job_button("Submit for review", "Submitting…", running(JobKind::AppStoreReview), false, "btn btn-publish",
                                move |_| jobs::submit_for_review(ws))}
                        })}
                }
            }

            if plat.has_android() {
                div { class: "card platform-card",
                    h2 { class: "build-android-title", {icon_play()} "Google Play" }
                    ul { class: "check-list",
                        {check_row(ws, !p.android_bundle_id.trim().is_empty() && creds.play_ok(), "Account",
                            if creds.play_ok() { p.android_bundle_id.clone() } else { "Service account missing".into() }, Some(Step::Accounts))}
                        {check_row(ws, described(true) == p.locales.len(), "Store text",
                            format!("{} of {} languages described", described(true), p.locales.len()), Some(Step::Metadata))}
                        {check_row(ws, !android_shots.is_empty(), "Screenshots",
                            format!("{} files", android_shots.len()), Some(Step::Screenshots))}
                        {check_row(ws, artifacts.aab.is_some(), "AAB",
                            artifacts.aab.as_ref().map(|a| a.summary()).unwrap_or_else(|| "Not built yet".into()), Some(Step::Build))}
                        if artifacts.aab.is_some() {
                            {build_check_row(ws, ws.aab_check.read().clone())}
                        }
                    }

                    {release_stage("1 · Listing",
                        "Store text, phone screenshots and feature graphic for every language.".into(),
                        ws.job_state(JobKind::GooglePlay), "Ready",
                        job_button("Upload listing", "Uploading…", running(JobKind::GooglePlay), false, "btn stage-btn",
                            move |_| jobs::publish_google_play(ws)))}

                    {release_stage("2 · Release",
                        "Uploads the newest AAB and puts it on a track, with What's new as release notes.".into(),
                        ws.job_state(JobKind::PlayRelease), "Ready",
                        rsx! {
                            div { class: "release-options",
                                select {
                                    class: "text-input",
                                    value: "{p.play_track}",
                                    onchange: move |e: Event<FormData>| ws.update(|p| p.play_track = e.value()),
                                    for (value, label) in [("internal", "Internal test"), ("alpha", "Closed test"), ("beta", "Open test"), ("production", "Production")] {
                                        option { value: "{value}", selected: p.play_track == value, "{label}" }
                                    }
                                }
                                select {
                                    class: "text-input",
                                    value: "{p.play_release_status}",
                                    onchange: move |e: Event<FormData>| ws.update(|p| p.play_release_status = e.value()),
                                    option { value: "draft", selected: p.play_release_status == "draft", "Draft" }
                                    option { value: "completed", selected: p.play_release_status == "completed", "Roll out" }
                                }
                            }
                            p { class: "settings-hint", "Apps that were never published only accept Draft." }
                            {job_button("Release AAB", "Releasing…", running(JobKind::PlayRelease), artifacts.aab.is_none(), "btn btn-publish btn-publish-android",
                                move |_| jobs::release_aab(ws))}
                        })}
                }
            }
        }

        StoreStatusCard {}

        div { class: "card",
            h2 { "History" }
            if p.releases.is_empty() {
                p { class: "output-empty", "Uploads and releases will be listed here." }
            } else {
                table { class: "history",
                    thead {
                        tr { th { "When (UTC)" } th { "Store" } th { "Version" } th { "What" } }
                    }
                    tbody {
                        for r in p.releases.iter().take(20) {
                            tr {
                                td { "{r.at}" }
                                td { if r.platform == "ios" { "App Store" } else { "Google Play" } }
                                td { if r.build > 0 { "{r.version} ({r.build})" } else { "{r.version}" } }
                                td { "{r.action}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
