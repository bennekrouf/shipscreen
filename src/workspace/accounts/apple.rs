//! The Apple account: signing identities and the App Store Connect key.

use super::*;

#[component]
pub(crate) fn AppleCard() -> Element {
    let ws = use_context::<Ws>();
    let mut settings = ws.settings;
    let p = ws.proj.read().clone();

    let mut identities = use_signal(Vec::<String>::new);
    let mut profiles = use_signal(Vec::<ProfileInfo>::new);
    use_effect(move || {
        ws.refresh.read(); // re-scan after a certificate or profile is created
        spawn(async move {
            identities.set(
                tokio::task::spawn_blocking(discover_identities)
                    .await
                    .unwrap_or_default(),
            );
            profiles.set(
                tokio::task::spawn_blocking(discover_profiles)
                    .await
                    .unwrap_or_default(),
            );
        });
    });

    let identity = settings.read().apple_identity.clone();
    let bundle_id = p.ios_bundle_id.trim().to_string();
    let creds = ws.creds.read().clone();
    let selected = ws.profile.read().clone();
    let profile_ok = selected.as_ref().is_some_and(|i| {
        profile_checks(i, &bundle_id, &identity)
            .iter()
            .all(|c| c.ok)
    });
    let has_distribution = identities
        .read()
        .iter()
        .any(|i| i.starts_with("Apple Distribution"));

    // Profiles for this app first, then App Store ones, then by name.
    let mut sorted = profiles.read().clone();
    sorted.sort_by(|a, b| {
        b.matches_bundle(&bundle_id)
            .cmp(&a.matches_bundle(&bundle_id))
            .then((b.kind == ProfileKind::AppStore).cmp(&(a.kind == ProfileKind::AppStore)))
            .then(a.name.cmp(&b.name))
    });

    // New-certificate form: the name defaults to the one in the identity.
    let default_name = identity
        .strip_prefix("Apple Distribution: ")
        .or_else(|| identity.strip_prefix("Apple Development: "))
        .and_then(|s| s.rsplit_once(" (").map(|(n, _)| n.to_string()))
        .unwrap_or_default();
    let mut cert_name = use_signal(|| default_name.clone());
    let mut cert_email = use_signal(String::new);
    let cert_pw = use_signal(String::new);
    let cert_pw2 = use_signal(String::new);
    let mut cert_error = use_signal(|| Option::<String>::None);
    let cert_state = ws.cert_job.read().clone();
    let profile_state = ws.profile_job.read().clone();
    let asc_ok = creds.app_store_ok();

    rsx! {
        div { class: "card",
            h2 { "Apple" }

            if !cfg!(target_os = "macos") {
                p { class: "settings-hint",
                    "Signing identities, certificates and provisioning profiles live in the macOS keychain and Xcode, so they are set up on the Mac that builds for iOS. The App Store Connect key below works here: it uploads the listing and screenshots."
                }
            } else {
                div { class: "settings-field",
                    label { "Signing identity" }
                    if identities.read().is_empty() {
                        p { class: "settings-hint hint-error", "No signing identity in the keychain — create a Distribution certificate below." }
                    } else {
                        select {
                            class: "text-input",
                            value: "{identity}",
                            onchange: move |e: Event<FormData>| { settings.write().apple_identity = e.value(); save_settings(&settings()); },
                            if identity.is_empty() {
                                option { value: "", disabled: true, selected: true, "— choose identity —" }
                            }
                            for id in identities.read().iter() {
                                option { value: "{id}", selected: *id == identity, "{id}" }
                            }
                        }
                        p { class: "settings-hint", "From the keychain · shared by every project on this Mac" }
                    }
                }

                details { class: "inline-form", open: !has_distribution,
                    summary {
                        if has_distribution { "Create another Distribution certificate" } else { "Create a Distribution certificate" }
                    }
                    if has_distribution {
                        p { class: "settings-hint hint-error",
                            "You already have a Distribution certificate in the keychain — choose it above. A new one is only needed if that one is lost or revoked: Apple allows very few per team, and your profiles are tied to the current one."
                        }
                    }
                    p { class: "settings-hint",
                        "Makes a new private key on this Mac, asks Apple for the certificate, installs it in the keychain and keeps a password-protected .p12 backup in your keys folder. Needs the App Store Connect key below."
                    }
                    div { class: "build-config-grid",
                        div { class: "build-config-field",
                            label { class: "build-config-label", "Your name" }
                            input { class: "text-input", placeholder: "As on your Apple Developer account", value: "{cert_name}", oninput: move |e| cert_name.set(e.value()) }
                        }
                        div { class: "build-config-field",
                            label { class: "build-config-label", "Apple ID email" }
                            input { class: "text-input", r#type: "email", placeholder: "The Apple ID you sign in to Apple Developer with", value: "{cert_email}", oninput: move |e| cert_email.set(e.value()) }
                        }
                        div { class: "build-config-field",
                            label { class: "build-config-label", ".p12 backup password" }
                            {password_input(cert_pw, "New password, at least 8 characters")}
                        }
                        div { class: "build-config-field",
                            label { class: "build-config-label", "Repeat password" }
                            {password_input(cert_pw2, "")}
                        }
                    }
                    p { class: "settings-hint",
                        "The name and email go into the certificate request, as Apple expects. The password is new: it protects the .p12 backup, which is what you'd import on another Mac — keep it in your password manager."
                    }
                    if let Some(e) = cert_error() {
                        p { class: "settings-hint hint-error", "{e}" }
                    }
                    div { class: "conn-test",
                        button {
                            class: "btn btn-sm",
                            disabled: matches!(cert_state, JobState::Running(_)) || !asc_ok,
                            onclick: move |_| {
                                let (pw, pw2) = (cert_pw.peek().clone(), cert_pw2.peek().clone());
                                if cert_name.peek().trim().is_empty() || !cert_email.peek().contains('@') {
                                    cert_error.set(Some("Fill in your name and Apple ID email.".into()));
                                } else if pw != pw2 {
                                    cert_error.set(Some("The passwords don't match.".into()));
                                } else {
                                    cert_error.set(None);
                                    jobs::create_distribution_certificate(ws, cert_name.peek().trim().into(), cert_email.peek().trim().into(), pw);
                                }
                            },
                            "Create certificate"
                        }
                        div { class: "publish-status-row", {job_status_line(cert_state.clone(), if asc_ok { "" } else { "Set up the App Store Connect key first" })} }
                    }
                }

                div { class: "settings-field",
                    label { "Provisioning profile" }
                    select {
                        class: "text-input",
                        value: "{p.provisioning_profile}",
                        onchange: move |e: Event<FormData>| ws.update(|p| p.provisioning_profile = e.value()),
                        if p.provisioning_profile.is_empty() {
                            option { value: "", disabled: true, selected: true, "— choose profile —" }
                        }
                        if !p.provisioning_profile.is_empty() && !sorted.iter().any(|i| i.path == p.provisioning_profile) {
                            option { value: "{p.provisioning_profile}", selected: true, "{p.provisioning_profile}" }
                        }
                        for info in sorted.iter() {
                            option {
                                value: "{info.path}",
                                selected: info.path == p.provisioning_profile,
                                if info.matches_bundle(&bundle_id) {
                                    "{info.name} · {info.kind.label()} · {info.app_id}"
                                } else {
                                    "{info.name} · {info.kind.label()} · {info.app_id} (other app)"
                                }
                            }
                        }
                    }
                    div { class: "install-actions",
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                spawn(async move {
                                    let Some(path) = pick_file("Add a provisioning profile", ("Provisioning profile", &["mobileprovision"])).await else {
                                        return;
                                    };
                                    let mut state = ws.profile_job;
                                    let installed = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| install_profile(&b));
                                    match installed.and_then(|p| read_profile(&p).map(|i| (p, i)).ok_or("Installed, but it can't be read".to_string())) {
                                        Ok((installed, info)) => {
                                            ws.update(|p| p.provisioning_profile = installed.to_string_lossy().to_string());
                                            let mut refresh = ws.refresh;
                                            refresh.with_mut(|n| *n += 1);
                                            state.set(JobState::Ok(format!("{} installed and selected", info.name)));
                                        }
                                        Err(e) => state.set(JobState::Failed(e)),
                                    }
                                });
                            },
                            "Add profile…"
                        }
                        a { class: "btn btn-sm", href: "https://developer.apple.com/account/resources/profiles/list", target: "_blank", "Profiles on Apple Developer" }
                    }
                    p { class: "settings-hint",
                        "Per project — profiles belong to one bundle ID. Ones matching this app are listed first. Downloaded one from Apple Developer? Add it here: it's installed for Xcode and the build too."
                    }
                }

                if let Some(info) = selected.as_ref() {
                    ul { class: "check-list",
                        for c in profile_checks(info, &bundle_id, &identity) {
                            {check_row(ws, c.ok, c.label, c.detail, None)}
                        }
                    }
                }
                if !profile_ok {
                    div { class: "conn-test",
                        button {
                            class: "btn btn-sm",
                            disabled: matches!(profile_state, JobState::Running(_)) || !asc_ok,
                            onclick: move |_| jobs::repair_profile(ws),
                            "Regenerate App Store profile"
                        }
                        div { class: "publish-status-row",
                            {job_status_line(profile_state.clone(), "Creates a fresh profile for this app on your current certificate and selects it")}
                        }
                    }
                }
            }

            p { class: "export-section-label", "App Store Connect API key" }
            ul { class: "check-list",
                for c in creds.app_store.iter().cloned() {
                    {check_row(ws, c.ok, c.label, c.detail, None)}
                }
            }
            if asc_ok {
                details { class: "inline-form",
                    summary { "Use a different key" }
                    AscKeySetup {}
                }
            } else {
                AscKeySetup {}
            }
            {connection_test(ws.asc_test.read().clone(), move |_| jobs::test_app_store(ws))}
        }
    }
}

/// Get the three App Store Connect values into the project without hunting:
/// reuse another project's key, or walk through creating one with the page
/// links, picking up the downloaded .p8 and its Key ID automatically.
#[component]
pub(crate) fn AscKeySetup() -> Element {
    let ws = use_context::<Ws>();
    let mut others = use_signal(Vec::<asckey::AscKey>::new);
    let mut p8s = use_signal(Vec::<(PathBuf, String)>::new);
    let mut key_id = use_signal(String::new);
    let mut issuer = use_signal(String::new);
    let mut p8 = use_signal(|| Option::<PathBuf>::None);
    let mut state = use_signal(|| JobState::Idle);

    use_effect(move || {
        ws.refresh.read();
        let projects = ws.settings.peek().recent_projects.clone();
        let (dir, keys) = (ws.dir(), keys_dir(&ws.settings.peek()));
        spawn(async move {
            let (o, f) = tokio::task::spawn_blocking(move || {
                (
                    asckey::from_other_projects(&projects, &dir),
                    asckey::find_p8s(&keys),
                )
            })
            .await
            .unwrap_or_default();
            // The Issuer ID is the same for every key of the team.
            if issuer.peek().is_empty() {
                if let Some(k) = o.first() {
                    issuer.set(k.issuer_id.clone());
                }
            }
            // A single downloaded key is almost certainly the one just made.
            if p8.peek().is_none() && f.len() == 1 {
                key_id.set(f[0].1.clone());
                p8.set(Some(f[0].0.clone()));
            }
            others.set(o);
            p8s.set(f);
        });
    });

    // Save into the project's .env, then prove it works.
    let mut save = move |key: asckey::AscKey| {
        let dir = ws.dir();
        let keys = keys_dir(&ws.settings.peek());
        let result = asckey::keep_in_keys_dir(&key.p8, &key.key_id, &keys).and_then(|kept| {
            let path = kept.to_string_lossy().to_string();
            envfile::set_values(
                &dir,
                &[
                    (asckey::KEY_ID_VAR, &key.key_id),
                    (asckey::ISSUER_VAR, &key.issuer_id),
                    (asckey::P8_VAR, &path),
                ],
            )
            .map(|_| kept)
        });
        match result {
            Ok(kept) => {
                state.set(JobState::Ok(format!(
                    "Saved in the project's .env · key kept at {}",
                    kept.display()
                )));
                let mut refresh = ws.refresh;
                refresh.with_mut(|n| *n += 1);
                jobs::test_app_store(ws);
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    };

    let (kid, iss) = (key_id(), issuer());
    let problem = if p8().is_none() {
        Some("Choose the downloaded .p8 key")
    } else if !asckey::valid_key_id(kid.trim()) {
        Some("The Key ID is 10 capital letters and digits")
    } else if !asckey::valid_issuer(iss.trim()) {
        Some("The Issuer ID looks like 69a6de97-00e6-47e3-e053-5b8c7c11a4d1")
    } else {
        None
    };

    rsx! {
        if !others.read().is_empty() {
            p { class: "settings-hint", "One key works for every app of your team — this Mac already has one:" }
            for k in others.read().iter().cloned() {
                div { class: "install-option",
                    div { class: "check-text",
                        span { class: "check-label", "Key {k.key_id}" }
                        span { class: "check-detail", "Used by {k.from} · {k.p8.display()}" }
                    }
                    div { class: "install-actions",
                        button { class: "btn btn-sm", onclick: move |_| save(k.clone()), "Use this key" }
                    }
                }
            }
        }
        details { class: "inline-form", open: others.read().is_empty(),
            summary { if others.read().is_empty() { "Set up a key" } else { "Or create a new key" } }
            ol { class: "lost-steps",
                li {
                    "Open "
                    a { href: asckey::KEYS_PAGE, target: "_blank", "App Store Connect → Users and Access → Integrations" }
                    ". The first time, the Account Holder has to click Request Access."
                }
                li {
                    "Click + to generate a key. Name it ShipScreen. Access: Admin lets ShipScreen also create certificates and profiles; App Manager is enough for uploads and review."
                }
                li { "Download the key right away — Apple lets you download it only once. ShipScreen keeps a copy in your keys folder." }
                li { "The Issuer ID is shown above the list of keys on the same page." }
            }
            div { class: "build-config-grid",
                div { class: "build-config-field",
                    label { class: "build-config-label", "Private key (.p8)" }
                    if p8s.read().len() > 1 {
                        select {
                            class: "text-input",
                            onchange: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Some((path, id)) = p8s.peek().iter().find(|(p, _)| p.to_string_lossy() == v).cloned() {
                                    key_id.set(id);
                                    p8.set(Some(path));
                                }
                            },
                            option { value: "", selected: p8().is_none(), disabled: true, "— downloaded keys —" }
                            for (path, id) in p8s.read().iter() {
                                option { value: "{path.display()}", selected: p8().as_ref() == Some(path), "{id} · {path.display()}" }
                            }
                        }
                    } else if let Some(path) = p8() {
                        p { class: "settings-hint", "{path.display()}" }
                    }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(path) = pick_file("Choose the App Store Connect key", ("App Store Connect key", &["p8"])).await {
                                    if let Some(id) = asckey::key_id_from_file(&path) {
                                        key_id.set(id);
                                    }
                                    p8.set(Some(path));
                                }
                            });
                        },
                        "Choose .p8…"
                    }
                }
                div { class: "build-config-field",
                    label { class: "build-config-label", "Key ID" }
                    input { class: "text-input", placeholder: "From the file name, e.g. 4TALSPNY5Y", value: "{kid}", oninput: move |e| key_id.set(e.value().trim().to_uppercase()) }
                }
                div { class: "build-config-field",
                    label { class: "build-config-label", "Issuer ID" }
                    input { class: "text-input", placeholder: "Above the keys list", value: "{iss}", oninput: move |e| issuer.set(e.value().trim().to_string()) }
                }
            }
            div { class: "conn-test",
                button {
                    class: "btn btn-sm",
                    disabled: problem.is_some(),
                    onclick: move |_| {
                        if let Some(path) = p8() {
                            save(asckey::AscKey { key_id: key_id().trim().into(), issuer_id: issuer().trim().into(), p8: path, from: String::new() });
                        }
                    },
                    "Save and test"
                }
                if let Some(pb) = problem {
                    span { class: "settings-hint", "{pb}" }
                }
            }
            p { class: "settings-hint",
                a { href: asckey::HELP_PAGE, target: "_blank", "Apple's guide to API keys" }
            }
        }
        div { class: "publish-status-row", {job_status_line(state(), "")} }
    }
}
