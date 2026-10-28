//! The upload key, the secrets, and backing them up.

use super::*;

#[component]
pub(crate) fn UploadKeyCard() -> Element {
    let ws = use_context::<Ws>();
    let mut settings = ws.settings;
    let mut refresh = ws.refresh;
    let p = ws.proj.read().clone();
    let android = ws.android.read().clone();
    let keystore_setting = settings.read().android_keystore_path.clone();
    let slug_alias = p.project_slug.trim().to_lowercase();

    let password = use_signal(String::new);
    let mut save_state = use_signal(|| JobState::Idle);

    let mut dn_name = use_signal(String::new);
    let mut dn_org = use_signal(String::new);
    let mut dn_country = use_signal(String::new);
    let new_pw = use_signal(String::new);
    let new_pw2 = use_signal(String::new);
    let mut create_state = use_signal(|| JobState::Idle);

    let keystore_path = android.keystore.clone();
    let has_keystore = keystore_path.as_ref().is_some_and(|k| k.is_file());
    // Keystores already on this Mac, offered while none is set: the keys folder and the project.
    let found = if has_keystore || android.keystore_from_env {
        Vec::new()
    } else {
        signing::find_keystores(&[keys_dir(&settings.read()), ws.dir()])
    };

    rsx! {
        div { class: "card",
            h2 { "Android upload key" }
            p { class: "hint card-hint", "Signs the AABs you upload. One keystore for all your apps, one key (alias) per app." }

            div { class: "build-config-grid",
                div { class: "build-config-field",
                    label { class: "build-config-label", "Keystore (all projects)" }
                    div { class: "settings-path-row",
                        input {
                            class: "text-input",
                            placeholder: "~/keys/upload.jks",
                            value: "{keystore_setting}",
                            disabled: android.keystore_from_env,
                            oninput: move |e: Event<FormData>| { settings.write().android_keystore_path = e.value(); save_settings(&settings()); },
                        }
                        button {
                            class: "btn settings-browse-btn",
                            disabled: android.keystore_from_env,
                            onclick: move |_| {
                                spawn(async move {
                                    if let Some(path) = pick_file("Choose upload keystore", ("Keystore", &["jks", "keystore", "p12"])).await {
                                        settings.write().android_keystore_path = path.to_string_lossy().to_string();
                                        save_settings(&settings());
                                    }
                                });
                            },
                            "Browse…"
                        }
                    }
                    if android.keystore_from_env {
                        p { class: "settings-hint", "Set by ANDROID_KEYSTORE_PATH in this project's .env, which takes priority." }
                    }
                }
                div { class: "build-config-field",
                    label { class: "build-config-label", "Key alias (this app)" }
                    input {
                        class: "text-input",
                        placeholder: "{slug_alias}",
                        value: "{p.android_key_alias}",
                        oninput: move |e: Event<FormData>| ws.update(|p| p.android_key_alias = e.value()),
                    }
                }
            }

            ul { class: "check-list",
                for c in android.checks() {
                    {check_row(ws, c.ok, c.label, c.detail, None)}
                }
            }

            if !found.is_empty() {
                div { class: "settings-field",
                    label { "Found on this Mac" }
                    for ks in found {
                        {
                            let path = ks.to_string_lossy().to_string();
                            rsx! {
                                div { key: "{path}", class: "settings-path-row",
                                    span { class: "settings-hint", "{path}" }
                                    button {
                                        class: "btn btn-sm",
                                        onclick: move |_| {
                                            settings.write().android_keystore_path = path.clone();
                                            save_settings(&settings());
                                            refresh.with_mut(|n| *n += 1);
                                        },
                                        "Use"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if has_keystore {
                div { class: "settings-field",
                    label { "Keystore password" }
                    div { class: "settings-path-row",
                        {password_input(password, if cfg!(windows) { "Checked against the keystore, then kept in Windows Credential Manager" } else { "Checked against the keystore, then kept in the Keychain" })}
                        button {
                            class: "btn settings-browse-btn",
                            disabled: matches!(save_state(), JobState::Running(_)),
                            onclick: move |_| {
                                let pw = password.peek().clone();
                                let ks = keystore_path.clone().unwrap_or_default();
                                let alias = ws.android.peek().alias.clone();
                                let mut password = password;
                                save_state.set(JobState::Running("Checking…".into()));
                                spawn(async move {
                                    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
                                        let aliases = signing::keystore_aliases(&ks, &pw)?;
                                        signing::keychain_set(&signing::keystore_account(&ks), &pw)?;
                                        Ok(if aliases.contains(&alias) {
                                            format!("Saved — the keystore has \"{alias}\"")
                                        } else {
                                            format!("Saved, but the keystore has no \"{alias}\" key ({}). Create one below or change the alias.", aliases.join(", "))
                                        })
                                    })
                                    .await
                                    .unwrap_or_else(|e| Err(e.to_string()));
                                    password.set(String::new());
                                    save_state.set(match result {
                                        Ok(m) => JobState::Ok(m),
                                        Err(e) => JobState::Failed(e),
                                    });
                                    refresh.with_mut(|n| *n += 1);
                                });
                            },
                            if cfg!(windows) { "Save to Credential Manager" } else { "Save to Keychain" }
                        }
                    }
                    div { class: "publish-status-row", {job_status_line(save_state(), "")} }
                }
            }

            details { class: "inline-form", open: !has_keystore,
                summary { if has_keystore { "Add a key for this app" } else { "Create an upload keystore" } }
                p { class: "settings-hint",
                    if has_keystore {
                        "Adds the \"{android.alias}\" key to your keystore (use its password) and exports the certificate Play Console asks for."
                    } else {
                        "Creates the keystore in your keys folder with the \"{android.alias}\" key, keeps the password in {signing::password_store_name()} and exports the certificate Play Console asks for."
                    }
                }
                div { class: "build-config-grid",
                    div { class: "build-config-field",
                        label { class: "build-config-label", "Name" }
                        input { class: "text-input", value: "{dn_name}", oninput: move |e| dn_name.set(e.value()) }
                    }
                    div { class: "build-config-field",
                        label { class: "build-config-label", "Organisation" }
                        input { class: "text-input", value: "{dn_org}", oninput: move |e| dn_org.set(e.value()) }
                    }
                    div { class: "build-config-field",
                        label { class: "build-config-label", "Country code" }
                        input { class: "text-input settings-short-input", placeholder: "CH", maxlength: 2, value: "{dn_country}", oninput: move |e| dn_country.set(e.value().to_uppercase()) }
                    }
                    div { class: "build-config-field",
                        label { class: "build-config-label", "Keystore password" }
                        {password_input(new_pw, "At least 6 characters")}
                    }
                    div { class: "build-config-field",
                        label { class: "build-config-label", "Repeat password" }
                        {password_input(new_pw2, "")}
                    }
                }
                div { class: "conn-test",
                    button {
                        class: "btn btn-sm",
                        disabled: matches!(create_state(), JobState::Running(_)),
                        onclick: move |_| {
                            let (pw, pw2) = (new_pw.peek().clone(), new_pw2.peek().clone());
                            if dn_name.peek().trim().is_empty() {
                                create_state.set(JobState::Failed("Fill in a name.".into()));
                                return;
                            }
                            if pw != pw2 {
                                create_state.set(JobState::Failed("The passwords don't match.".into()));
                                return;
                            }
                            let ks = ws.android.peek().keystore.clone().unwrap_or_else(|| keys_dir(&settings.peek()).join("upload.jks"));
                            let alias = ws.android.peek().alias.clone();
                            let dn = signing::dname(&dn_name.peek(), &dn_org.peek(), &dn_country.peek());
                            let (mut new_pw, mut new_pw2) = (new_pw, new_pw2);
                            create_state.set(JobState::Running("Creating key…".into()));
                            spawn(async move {
                                let ks2 = ks.clone();
                                let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
                                    let pem = signing::create_upload_key(&ks2, &alias, &pw, &dn)?;
                                    signing::keychain_set(&signing::keystore_account(&ks2), &pw)?;
                                    let fp = signing::upload_key_fingerprint(&ks2, &alias, &pw).unwrap_or_default();
                                    Ok(format!(
                                        "Created \"{alias}\" · SHA-256 {fp} · certificate: {} — for an app already on Play, upload it under App integrity → Request upload key reset; for a new app, the first upload registers it.",
                                        pem.display()
                                    ))
                                })
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()));
                                new_pw.set(String::new());
                                new_pw2.set(String::new());
                                if result.is_ok() && settings.peek().android_keystore_path.trim().is_empty() {
                                    settings.write().android_keystore_path = ks.to_string_lossy().to_string();
                                    save_settings(&settings.peek());
                                }
                                create_state.set(match result {
                                    Ok(m) => JobState::Ok(m),
                                    Err(e) => JobState::Failed(e),
                                });
                                refresh.with_mut(|n| *n += 1);
                            });
                        },
                        if has_keystore { "Add key" } else { "Create keystore" }
                    }
                    div { class: "publish-status-row", {job_status_line(create_state(), "")} }
                }
            }
        }
    }
}

#[component]
pub(crate) fn SecretsCard() -> Element {
    let ws = use_context::<Ws>();
    let mut refresh = ws.refresh;
    let tracked = ws.tracked_secrets.read().clone();
    let mut state = use_signal(|| JobState::Idle);

    rsx! {
        div { class: "card",
            h2 { "Secrets in git" }
            if tracked.is_empty() {
                ul { class: "check-list",
                    {check_row(ws, true, "No secret files tracked", "No .env, keys, keystores or service-account files are committed in this project".into(), None)}
                }
            } else {
                ul { class: "check-list",
                    for f in tracked.iter() {
                        {check_row(ws, false, f, "Tracked by git — anyone with the repository has it".into(), None)}
                    }
                }
                div { class: "conn-test",
                    button {
                        class: "btn btn-sm",
                        disabled: matches!(state(), JobState::Running(_)),
                        onclick: move |_| {
                            let files = ws.tracked_secrets.peek().clone();
                            let dir = ws.dir();
                            spawn(async move {
                                let r = tokio::task::spawn_blocking(move || signing::untrack_secrets(&dir, &files)).await.unwrap_or_else(|e| Err(e.to_string()));
                                state.set(match r {
                                    Ok(()) => JobState::Ok("Untracked and added to .gitignore — commit the change".into()),
                                    Err(e) => JobState::Failed(e),
                                });
                                refresh.with_mut(|n| *n += 1);
                            });
                        },
                        "Stop tracking them"
                    }
                    div { class: "publish-status-row", {job_status_line(state(), "")} }
                }
                p { class: "settings-hint",
                    "The files stay on disk. Copies already committed remain in the history, so treat those keys as exposed and replace them."
                }
            }
        }
    }
}

#[component]
pub(crate) fn BackupCard() -> Element {
    let ws = use_context::<Ws>();
    let mut settings = ws.settings;
    let mut refresh = ws.refresh;
    let backup = ws.backup.read().clone();
    let keys = keys_dir(&settings.read());
    let pw = use_signal(String::new);
    let pw2 = use_signal(String::new);
    let mut state = use_signal(|| JobState::Idle);

    let (ok, detail) = match &backup {
        BackupState::Never => (
            false,
            "No backup yet — if this Mac is lost, so are your keys".to_string(),
        ),
        BackupState::Outdated(at) => (
            false,
            format!("Last backup {at} UTC, but keys changed since"),
        ),
        BackupState::Current(at) => (
            true,
            format!(
                "Last backup {at} UTC · {}",
                settings.read().last_backup_path
            ),
        ),
    };

    rsx! {
        div { class: "card",
            h2 { "Backup" }
            ul { class: "check-list",
                {check_row(ws, ok, "Encrypted backup of your keys", detail, None)}
            }
            div { class: "build-config-grid",
                div { class: "build-config-field",
                    label { class: "build-config-label", "Keys folder" }
                    input {
                        class: "text-input",
                        placeholder: "{signing::default_keys_dir().display()}",
                        value: "{settings.read().keys_dir}",
                        oninput: move |e: Event<FormData>| { settings.write().keys_dir = e.value(); save_settings(&settings()); },
                    }
                    p { class: "settings-hint", "Upload keystore, .p12, .p8 and service-account files — outside every project." }
                }
                div { class: "build-config-field",
                    label { class: "build-config-label", "Backup password" }
                    {password_input(pw, "At least 8 characters")}
                }
                div { class: "build-config-field",
                    label { class: "build-config-label", "Repeat password" }
                    {password_input(pw2, "")}
                }
            }
            div { class: "conn-test",
                button {
                    class: "btn btn-sm",
                    disabled: matches!(state(), JobState::Running(_)),
                    onclick: move |_| {
                        let (p1, p2) = (pw.peek().clone(), pw2.peek().clone());
                        if p1 != p2 {
                            state.set(JobState::Failed("The passwords don't match.".into()));
                            return;
                        }
                        let sources = backup_sources(&settings.peek());
                        let (mut pw, mut pw2) = (pw, pw2);
                        spawn(async move {
                            let name = format!("shipscreen-keys-{}.enc", &utc_now()[..10]);
                            let Some(dest) = save_file("Save encrypted backup", &name).await else { return };
                            state.set(JobState::Running("Encrypting…".into()));
                            let dest2 = dest.clone();
                            let r = tokio::task::spawn_blocking(move || signing::export_backup(&sources, &dest2, &p1)).await.unwrap_or_else(|e| Err(e.to_string()));
                            pw.set(String::new());
                            pw2.set(String::new());
                            match r {
                                Ok(()) => {
                                    settings.write().last_backup_at = utc_now();
                                    settings.write().last_backup_path = dest.to_string_lossy().to_string();
                                    save_settings(&settings.peek());
                                    state.set(JobState::Ok("Saved — keep it off this Mac (USB drive, password manager, cloud)".into()));
                                }
                                Err(e) => state.set(JobState::Failed(e)),
                            }
                            refresh.with_mut(|n| *n += 1);
                        });
                    },
                    "Export encrypted backup…"
                }
                div { class: "publish-status-row", {job_status_line(state(), "")} }
            }
            p { class: "settings-hint",
                "Covers {keys.display()}"
                if !settings.read().android_keystore_path.trim().is_empty() { " and your upload keystore" }
                ". Restore with: openssl enc -d -aes-256-cbc -pbkdf2 -in FILE | tar xz"
            }
        }
    }
}

/// The checklist from the day a Mac is stolen: what to revoke and replace,
/// filled in with this project's own key IDs.
#[component]
pub(crate) fn LostMacCard() -> Element {
    let ws = use_context::<Ws>();
    let p = ws.proj.read().clone();
    let plat = p.platform_type.clone();
    let resolve = env_lookup(&ws.dir());
    let asc_key = resolve("APP_STORE_CONNECT_API_KEY_KEY_ID").unwrap_or_else(|| "(not set)".into());
    let sa = resolve("GOOGLE_PLAY_JSON_KEY").and_then(|p| service_account_ids(&p));
    let identity = ws.settings.read().apple_identity.clone();
    let alias = ws.android.read().alias.clone();

    rsx! {
        details { class: "card inline-form",
            summary { "If this Mac is lost or stolen" }
            p { class: "settings-hint", "In this order — the first ones let someone publish as you." }
            ol { class: "lost-steps",
                li {
                    "Sign the Mac out of your Apple ID and Google accounts, and mark it lost in "
                    a { href: "https://www.icloud.com/find", target: "_blank", "Find My" }
                    "."
                }
                if plat.has_ios() {
                    li {
                        "Revoke the App Store Connect API key "
                        code { "{asc_key}" }
                        " and create a new one: "
                        a { href: "https://appstoreconnect.apple.com/access/integrations/api", target: "_blank", "Integrations" }
                        "."
                    }
                }
                if plat.has_android() {
                    li {
                        "Delete the Google service-account key "
                        code { if let Some((_, k)) = &sa { "{k}…" } else { "(not set)" } }
                        " and create a new one: "
                        a {
                            href: if let Some((proj, _)) = &sa { format!("https://console.cloud.google.com/iam-admin/serviceaccounts?project={proj}") } else { "https://console.cloud.google.com/iam-admin/serviceaccounts".into() },
                            target: "_blank",
                            "Service accounts"
                        }
                        "."
                    }
                }
                if plat.has_ios() {
                    li {
                        "Revoke the Distribution certificate ("
                        if identity.is_empty() { "none selected" } else { "{identity}" }
                        ") in "
                        a { href: "https://developer.apple.com/account/resources/certificates/list", target: "_blank", "Certificates" }
                        ", then create a new one and regenerate the profile above."
                    }
                    li {
                        "Developer ID certificates (macOS apps): don't revoke them yourself — ask "
                        a { href: "https://developer.apple.com/contact/", target: "_blank", "Apple Developer Support" }
                        " to revoke them from the theft date, so apps already downloaded keep opening."
                    }
                }
                if plat.has_android() {
                    li {
                        "Create a new upload key (\"{alias}\") above and request an upload key reset in "
                        a { href: "https://play.google.com/console", target: "_blank", "Play Console" }
                        " → App integrity → App signing. Apps never uploaded need nothing: the first upload registers the new key."
                    }
                }
                li { "Restore the rest from your encrypted backup, then make a new one." }
            }
        }
    }
}
