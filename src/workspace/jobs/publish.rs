//! Jobs that talk to the stores: uploads, submissions, releases and their
//! status.

use super::*;

pub(crate) fn publish_app_store(ws: Ws) {
    let proj = ws.proj;
    let mut publish_phase = ws.ios_pub;
    let mut publish_log = ws.ios_pub_log;
    let proj_dir = ws.dir();

    publish_phase.set(PublishPhase::Running);
    publish_log.set(Vec::new());
    ws.show_job(JobKind::AppStore);

    // Snapshot project state needed inside the async block.
    let bundle_id = proj.read().ios_bundle_id.clone(); // ASC lookup uses iOS bundle ID
    let locales = proj.read().locales.clone();
    let output_paths = proj.read().output_paths.clone();
    let app_name = proj.read().app_name.clone();
    let store_texts = proj.read().store_texts.clone();
    let support_url = proj.read().support_url.clone();
    let ios_version = proj.read().version.trim().to_string();
    if !valid_version(&ios_version) {
        publish_phase.set(PublishPhase::Error(
            "Set a valid version (e.g. 1.2.0) in the Version step.".into(),
        ));
        ws.show_job(JobKind::AppStore);
        return;
    }
    let proj_dir = proj_dir.clone();
    let fastlane_ios = proj_dir.join("fastlane").join("screenshots").join("ios");

    ws.spawn(async move {
            let mut log = publish_log;
            let mut push = |msg: &str| log.write().push(msg.to_string());

            // ── 1. Credentials → App Store Connect JWT ────────────────────
            push("🔑 Authenticating with App Store Connect…");
            let jwt = match asc_auth(&proj_dir).await {
                Ok(t) => t,
                Err(e) => { publish_phase.set(PublishPhase::Error(e)); return; }
            };
            push("✅ Authenticated");

            // ── 3. Find the app by bundle ID ──────────────────────────────
            push(&format!("🔍 Looking up app: {bundle_id}"));
            let app_id = match asc_find_app(&jwt, &bundle_id).await {
                Ok(id) => id,
                Err(e) => { publish_phase.set(PublishPhase::Error(format!("App lookup failed: {e}"))); return; }
            };
            push(&format!("   App ID: {app_id}"));

            // ── 4. Find or create the latest editable version ─────────────
            push("🔍 Finding (or creating) app version…");
            let version_id = match asc_find_or_create_version(&jwt, &app_id, &ios_version).await {
                Ok(id) => id,
                Err(e) => { publish_phase.set(PublishPhase::Error(format!("Version lookup failed: {e}"))); return; }
            };
            push(&format!("   Version ID: {version_id}"));

            // ── 5. Get localizations for this version ─────────────────────
            push("🌐 Fetching localizations…");
            let mut loc_map = match asc_get_localizations(&jwt, &version_id).await {
                Ok(m) => m,
                Err(e) => { publish_phase.set(PublishPhase::Error(format!("Localization fetch failed: {e}"))); return; }
            };
            push(&format!("   Found localizations: {}", loc_map.keys().cloned().collect::<Vec<_>>().join(", ")));

            // ── 5b. Ensure every localization has the correct app name ─────
            if !app_name.is_empty() {
                push(&format!("✏️  Setting app name to \"{app_name}\" on all localizations…"));
                for (locale, loc_id) in &loc_map {
                    if let Err(e) = asc_set_localization_name(&jwt, loc_id, &app_name).await {
                        push(&format!("   ⚠️  Could not set name for {locale}: {e}"));
                    }
                }
            }

            // ── 5c. Store text; add languages App Store Connect lacks ─────
            for locale in &locales {
                let text = store_texts.get(locale).cloned().unwrap_or_default();
                match loc_map.get(locale.as_str()).cloned() {
                    Some(loc_id) => {
                        let attrs = stores::asc_localization_attributes(&text, &support_url, true);
                        if attrs.as_object().is_some_and(|o| o.is_empty()) {
                            continue;
                        }
                        push(&format!("📝 [{locale}] Updating store text…"));
                        if let Err(e) = stores::asc_update_localization(&jwt, &loc_id, attrs).await {
                            // "What's New" is refused on an app's first version;
                            // save everything else rather than failing the lot.
                            let without = stores::asc_localization_attributes(&text, &support_url, false);
                            if !text.whats_new.trim().is_empty()
                                && stores::asc_update_localization(&jwt, &loc_id, without).await.is_ok()
                            {
                                push(&format!("   ⚠️  [{locale}] What's New not accepted (first version?) — the rest was saved"));
                            } else {
                                publish_phase.set(PublishPhase::Error(format!("[{locale}] Store text: {e}")));
                                return;
                            }
                        }
                    }
                    None => {
                        push(&format!("🌐 [{locale}] Adding language to App Store Connect…"));
                        let attrs = stores::asc_localization_attributes(&text, &support_url, false);
                        match stores::asc_create_localization(&jwt, &version_id, locale, attrs).await {
                            Ok(id) => {
                                loc_map.insert(locale.clone(), id);
                            }
                            Err(e) => push(&format!("   ⚠️  [{locale}] Could not add language: {e}")),
                        }
                    }
                }
            }

            // ── 6. For each locale × device type: upload screenshots ──────
            // Map IOS_TARGETS device folder names to ASC screenshotDisplayType values.
            // https://developer.apple.com/documentation/appstoreconnectapi/screenshotdisplaytype
            let device_types: &[(&str, &str)] = &[
                ("iPhone 6.9\" Display", "APP_IPHONE_69"),
                ("iPhone 6.7\" Display", "APP_IPHONE_67"),
                ("iPhone 6.5\" Display", "APP_IPHONE_65"),
                ("iPad Pro (12.9-inch)", "APP_IPAD_PRO_3GEN_129"),
            ];

            let upload_locales: Vec<String> = if locales.is_empty() {
                vec!["en-US".to_string()]
            } else {
                locales.clone()
            };

            for locale in &upload_locales {
                let loc_id = match loc_map.get(locale.as_str()) {
                    Some(id) => id.clone(),
                    None => {
                        push(&format!("⚠️  No localization found for {locale} — skipping (create it in App Store Connect first)"));
                        continue;
                    }
                };
                push(&format!("─── Locale: {locale} (loc_id: {loc_id}) ───"));

                for (device_name, display_type) in device_types {
                    // Collect screenshot files for this locale + device from output_paths,
                    // then fall back to the fastlane screenshots folder.
                    let mut files: Vec<PathBuf> = output_paths.iter()
                        .filter(|(label, path)| {
                            label.contains(device_name) && label.contains(locale.as_str()) && path.exists()
                        })
                        .map(|(_, p)| p.clone())
                        .collect();

                    if files.is_empty() {
                        // Fallback: fastlane/screenshots/ios/<locale>/<device_name>-*.png
                        let dir = fastlane_ios.join(locale);
                        if dir.exists() {
                            if let Ok(entries) = std::fs::read_dir(&dir) {
                                let mut found: Vec<PathBuf> = entries
                                    .flatten()
                                    .map(|e| e.path())
                                    .filter(|p| {
                                        p.file_name()
                                            .and_then(|n| n.to_str())
                                            .map(|n| n.starts_with(device_name) && n.ends_with(".png"))
                                            .unwrap_or(false)
                                    })
                                    .collect();
                                found.sort();
                                files = found;
                            }
                        }
                    }

                    if files.is_empty() {
                        push(&format!("   ⏭  No files for {device_name} [{locale}] — skipping"));
                        continue;
                    }

                    push(&format!("   📱 {device_name} — {} screenshot(s)", files.len()));

                    // Get or create screenshot set for this localization + display type.
                    let set_id = match asc_get_or_create_screenshot_set(&jwt, &loc_id, display_type).await {
                        Ok(id) => id,
                        Err(e) => {
                            publish_phase.set(PublishPhase::Error(format!("[{locale}] {device_name}: screenshot set error: {e}")));
                            return;
                        }
                    };

                    // Delete existing screenshots in the set before uploading fresh ones.
                    if let Err(e) = asc_delete_all_screenshots_in_set(&jwt, &set_id).await {
                        push(&format!("   ⚠️  Could not clear existing screenshots: {e}"));
                    }

                    // Upload each file.
                    for path in &files {
                        let fname = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        push(&format!("   ⬆  {fname}…"));
                        match asc_upload_screenshot(&jwt, &set_id, path).await {
                            Ok(_) => push(&format!("   ✅ {fname}")),
                            Err(e) => {
                                publish_phase.set(PublishPhase::Error(format!("[{locale}] {device_name} {fname}: {e}")));
                                return;
                            }
                        }
                    }
                }
            }

            push("🎉 App Store listing uploaded (store text + screenshots)");
            record_release(ws, "ios", &ios_version, 0, "Listing uploaded");
            publish_phase.set(PublishPhase::Success);
        });
}

pub(crate) fn publish_google_play(ws: Ws) {
    let proj = ws.proj;
    let mut android_publish_phase = ws.play_pub;
    let mut android_publish_log = ws.play_pub_log;
    let proj_dir = ws.dir();

    android_publish_phase.set(AndroidPublishPhase::Running);
    android_publish_log.set(Vec::new());
    ws.show_job(JobKind::GooglePlay);

    // Snapshot what we need from project state before moving into async.
    let bundle_id = proj.read().android_bundle_id.clone(); // Google Play uses Android package name
    let output_paths = proj.read().output_paths.clone();
    let store_texts = proj.read().store_texts.clone();
    let app_name = proj.read().app_name.clone();
    let version = proj.read().version.trim().to_string();

    let proj_dir = proj_dir.clone();
    ws.spawn(async move {
            let mut log = android_publish_log;
            let mut push = |msg: &str| log.write().push(msg.to_string());

            // ── 1. Credentials → Google access token ──────────────────────
            push("🔑 Authenticating with Google…");
            let (access_token, package_name) = match play_auth(&proj_dir, &bundle_id).await {
                Ok(v) => v,
                Err(e) => {
                    android_publish_phase.set(AndroidPublishPhase::Error(e));
                    return;
                }
            };
            push(&format!("📦 Package: {package_name}"));
            push("✅ Authenticated");

            // ── 4. Create an edit ─────────────────────────────────────────
            push("📝 Creating edit…");
            let edit_id = match google_play_create_edit(&access_token, &package_name).await {
                Ok(id) => id,
                Err(e) => {
                    android_publish_phase.set(AndroidPublishPhase::Error(
                        format!("Failed to create edit: {e}")
                    ));
                    return;
                }
            };
            push(&format!("   Edit ID: {edit_id}"));

            // ── 5. Collect Android screenshots from output_paths ──────────
            // output_paths entries: (label, path)
            // Labels look like "Android Phone [fr-FR] #1" / "Android Feature [fr-FR] #1"
            // (older output: "Screen 1 → Android Phone", no language tag)
            // Each language uploads its own tagged files (captions are baked in per language).
            let selected_locales = proj.read().locales.clone();
            let play_languages = if selected_locales.is_empty() {
                vec![proj.read().locale.clone()]
            } else {
                selected_locales.clone()
            };

            let android_files: Vec<(String, PathBuf)> = output_paths
                .iter()
                .filter(|(label, path)| {
                    (label.contains("Android Phone") || label.contains("Android Feature"))
                    && path.exists()
                })
                .map(|(label, path)| (label.clone(), path.clone()))
                .collect();

            let has_text = play_languages.iter().any(|l| store_texts.get(l).is_some_and(|t| !t.description.trim().is_empty()));
            if android_files.is_empty() && !has_text {
                android_publish_phase.set(AndroidPublishPhase::Error(
                    "Nothing to upload: no Android screenshots and no store text. Generate screenshots or fill in the Store text step.".into()
                ));
                let _ = google_play_delete_edit(&access_token, &package_name, &edit_id).await;
                return;
            }

            push(&format!("🖼  Found {} Android screenshot(s) to upload across {} locale(s)",
                android_files.len(), play_languages.len()));

            // ── 6. Upload each screenshot per locale ──────────────────────
            for play_language in &play_languages {
                // Play has its own codes for some languages (zh-CN, not zh-Hans).
                let api_lang = stores::play_language(play_language);
                push(&format!("─── Locale: {play_language} (Play: {api_lang}) ───"));

                let text = store_texts.get(play_language.as_str()).cloned().unwrap_or_default();
                if !text.description.trim().is_empty() && !text.short_description.trim().is_empty() {
                    push(&format!("📝 [{api_lang}] Updating store listing…"));
                    if let Err(e) = stores::play_update_listing(
                        &access_token, &package_name, &edit_id, api_lang, &app_name, &text,
                    ).await {
                        let _ = google_play_delete_edit(&access_token, &package_name, &edit_id).await;
                        android_publish_phase.set(AndroidPublishPhase::Error(format!("[{api_lang}] Listing: {e}")));
                        return;
                    }
                } else if !text.description.trim().is_empty() || !text.short_description.trim().is_empty() {
                    push(&format!("   ⚠️  [{api_lang}] Listing skipped — Play needs both a description and a short description"));
                }

                // Collect paths for this locale (prefer locale-specific files if they exist)
                let mut phone_paths: Vec<PathBuf> = Vec::new();
                let mut feature_paths: Vec<PathBuf> = Vec::new();

                // Files generated for this language carry its [locale] tag.
                let tag = format!("[{play_language}]");
                let locale_phone: Vec<PathBuf> = android_files.iter()
                    .filter(|(l, _)| l.contains("Android Phone") && l.contains(&tag))
                    .map(|(_, p)| p.clone()).collect();
                let locale_feature: Vec<PathBuf> = android_files.iter()
                    .filter(|(l, _)| l.contains("Android Feature") && l.contains(&tag))
                    .map(|(_, p)| p.clone()).collect();

                if !locale_phone.is_empty() || !locale_feature.is_empty() {
                    phone_paths = locale_phone;
                    feature_paths = locale_feature;
                } else {
                    // Output from before language tags existed is untagged and
                    // shared; never borrow another language's tagged files.
                    for (label, path) in android_files.iter().filter(|(l, _)| !l.contains('[')) {
                        if label.contains("Android Phone") { phone_paths.push(path.clone()); }
                        else if label.contains("Android Feature") { feature_paths.push(path.clone()); }
                    }
                }

                // Delete existing then upload for each image type
                for (image_type, paths) in [
                    ("PHONE_SCREENSHOTS", &phone_paths),
                    ("FEATURE_GRAPHIC", &feature_paths),
                ] {
                    if paths.is_empty() { continue; }

                    // Clear existing
                    let _ = google_play_delete_images(
                        &access_token, &package_name, &edit_id, api_lang, image_type
                    ).await;

                    for path in paths {
                        let fname = path.file_name().unwrap_or_default().to_string_lossy();
                        push(&format!("⬆  [{play_language}] Uploading {fname} ({image_type})…"));
                        if let Err(e) = google_play_upload_image(
                            &access_token, &package_name, &edit_id,
                            api_lang, image_type, path
                        ).await {
                            let _ = google_play_delete_edit(&access_token, &package_name, &edit_id).await;
                            android_publish_phase.set(AndroidPublishPhase::Error(
                                format!("[{play_language}] Upload failed for {fname}: {e}")
                            ));
                            return;
                        }
                        push(&format!("   ✅ {fname}"));
                    }
                }
            }

            // ── 7. Commit the edit ────────────────────────────────────────
            push("💾 Committing edit…");
            if let Err(e) = google_play_commit_edit(&access_token, &package_name, &edit_id).await {
                android_publish_phase.set(AndroidPublishPhase::Error(
                    format!("Failed to commit edit: {e}")
                ));
                return;
            }

            push("🎉 Google Play listing uploaded (store text + screenshots)");
            record_release(ws, "android", &version, 0, "Listing uploaded");
            android_publish_phase.set(AndroidPublishPhase::Success);
        });
}

/// Exchange the project's service account for a Google access token.
/// Returns (access token, package name).
pub(crate) async fn play_auth(
    dir: &std::path::Path,
    bundle_id: &str,
) -> Result<(String, String), String> {
    let resolve = env_resolver(dir);
    // GOOGLE_PLAY_JSON_KEY  – path to service-account .json file
    // ANDROID_PACKAGE_NAME  – optional override (falls back to bundle_id)
    let json_key_path = resolve("GOOGLE_PLAY_JSON_KEY").ok_or(
        "Missing GOOGLE_PLAY_JSON_KEY env var.\nSet it to the path of your Google service-account JSON file in fastlane/.env or export it before launching the app.",
    )?;
    let package_name = resolve("ANDROID_PACKAGE_NAME")
        .or_else(|| (!bundle_id.is_empty()).then(|| bundle_id.to_string()))
        .ok_or("Cannot determine Android package name.\nSet ANDROID_PACKAGE_NAME in fastlane/.env or fill in the Android package in the App step.")?;

    let sa_json: serde_json::Value = tokio::fs::read_to_string(&json_key_path)
        .await
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .map_err(|e| format!("Failed to read service-account JSON at {json_key_path}: {e}"))?;
    let client_email = sa_json["client_email"].as_str().unwrap_or("");
    let private_key_pem = sa_json["private_key"].as_str().unwrap_or("");
    if client_email.is_empty() || private_key_pem.is_empty() {
        return Err("Service-account JSON is missing client_email or private_key.".into());
    }
    let token = google_play_access_token(client_email, private_key_pem)
        .await
        .map_err(|e| format!("Authentication failed: {e}"))?;
    Ok((token, package_name))
}

/// Authenticate, then look the app up by bundle ID.
pub(crate) fn test_app_store(ws: Ws) {
    let mut state = ws.asc_test;
    let bundle_id = ws.proj.read().ios_bundle_id.trim().to_string();
    let dir = ws.dir();
    state.set(JobState::Running("Connecting…".into()));
    ws.spawn(async move {
        let result: Result<String, String> = async {
            let jwt = asc_auth(&dir).await?;
            if bundle_id.is_empty() {
                return Ok("Authenticated. Set the iOS bundle ID to check the app.".to_string());
            }
            let app_id = asc_find_app(&jwt, &bundle_id).await?;
            Ok(format!("Connected · {bundle_id} found (app {app_id})"))
        }
        .await;
        state.set(match result {
            Ok(m) => JobState::Ok(m),
            Err(e) => JobState::Failed(e),
        });
    });
}

/// Authenticate, then open and discard an edit — proves the service account
/// can actually change this app's listing.
pub(crate) fn test_google_play(ws: Ws) {
    let mut state = ws.play_test;
    let bundle_id = ws.proj.read().android_bundle_id.trim().to_string();
    let dir = ws.dir();
    state.set(JobState::Running("Connecting…".into()));
    ws.spawn(async move {
        let result: Result<String, String> = async {
            let (token, package) = play_auth(&dir, &bundle_id).await?;
            let edit_id = google_play_create_edit(&token, &package)
                .await
                .map_err(|e| format!("Authenticated, but cannot edit {package}: {e}"))?;
            let _ = google_play_delete_edit(&token, &package, &edit_id).await;
            Ok(format!("Connected · can edit {package}"))
        }
        .await;
        state.set(match result {
            Ok(m) => JobState::Ok(m),
            Err(e) => JobState::Failed(e),
        });
    });
}

/// Upload the newest IPA with `xcrun altool`, authenticated with the
/// project's App Store Connect API key.
pub(crate) fn upload_ipa(ws: Ws) {
    let mut state = ws.ios_build_job;
    let mut log = ws.ios_build_log;
    let dir = ws.dir();
    ws.show_job(JobKind::AppStoreBuild);
    let Some(ipa) = ws.artifacts.read().ipa.clone() else {
        state.set(JobState::Failed(
            "No IPA yet — build one in the Build step.".into(),
        ));
        return;
    };
    state.set(JobState::Running("Uploading IPA…".into()));
    log.set(Vec::new());

    ws.spawn(async move {
        let result: Result<(String, u32), String> = async {
            let (key_id, issuer_id, key_path) = asc_key(&dir)?;
            let ipa_path = ipa.path.clone();
            let (version, build) = tokio::task::spawn_blocking(move || stores::ipa_version(&ipa_path))
                .await
                .ok()
                .flatten()
                .ok_or("Could not read the version from the IPA's Info.plist")?;
            log.write().push(format!("📦 {} — version {version} ({build})", ipa.file_name()));

            // altool only finds keys named AuthKey_<id>.p8 in a known folder;
            // hand it a private temporary one rather than touching ~/.
            let keys_dir = std::env::temp_dir().join(format!("shipscreen-keys-{}", std::process::id()));
            std::fs::create_dir_all(&keys_dir).map_err(|e| e.to_string())?;
            let key_copy = keys_dir.join(format!("AuthKey_{key_id}.p8"));
            std::fs::copy(&key_path, &key_copy).map_err(|e| format!("Cannot read .p8 key at {key_path}: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_copy, std::fs::Permissions::from_mode(0o600));
            }

            log.write().push("⬆  Uploading with xcrun altool (this can take a few minutes)…".into());
            let mut cmd = std::process::Command::new("xcrun");
            cmd.args(["altool", "--upload-app", "--type", "ios", "--file"])
                .arg(&ipa.path)
                .args(["--apiKey", &key_id, "--apiIssuer", &issuer_id])
                .env("API_PRIVATE_KEYS_DIR", &keys_dir);
            let uploaded = stream_command(cmd, "altool", log).await;
            let _ = std::fs::remove_dir_all(&keys_dir);
            uploaded?;
            Ok((version, build))
        }
        .await;

        match result {
            Ok((version, build)) => {
                record_release(ws, "ios", &version, build, "Uploaded build");
                log.write().push("🎉 Uploaded. App Store Connect needs a few minutes to process it before it can be submitted.".into());
                state.set(JobState::Ok(format!("Uploaded {version} ({build})")));
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    });
}

pub(crate) fn submit_for_review(ws: Ws) {
    let mut state = ws.ios_review_job;
    let mut log = ws.ios_review_log;
    let dir = ws.dir();
    let bundle_id = ws.proj.read().ios_bundle_id.trim().to_string();
    let exempt = ws.proj.read().exempt_encryption;
    ws.show_job(JobKind::AppStoreReview);
    let Some((version, build)) = ios_build_to_submit(ws) else {
        state.set(JobState::Failed(
            "No build to submit — upload an IPA first.".into(),
        ));
        return;
    };
    state.set(JobState::Running(format!(
        "Submitting {version} ({build})…"
    )));
    log.set(Vec::new());

    ws.spawn(async move {
        let mut push = move |msg: String| log.write().push(msg);
        let result: Result<(), String> = async {
            push("🔑 Authenticating with App Store Connect…".into());
            let jwt = asc_auth(&dir).await?;
            let app_id = asc_find_app(&jwt, &bundle_id).await?;

            // A fresh upload takes a while to process; wait for it (≤ 30 min).
            push(format!("🔍 Looking for build {version} ({build})…"));
            let mut found = None;
            for attempt in 0..60 {
                match stores::asc_find_build(&jwt, &app_id, &version, build).await? {
                    Some(b) if b.state == "VALID" => {
                        found = Some(b);
                        break;
                    }
                    Some(b) if b.state == "FAILED" || b.state == "INVALID" => {
                        return Err(format!("App Store Connect marked build {build} as {} — check the email Apple sent.", b.state));
                    }
                    Some(_) => push(format!("   ⏳ Processing… (checked {} time(s))", attempt + 1)),
                    None => push(format!("   ⏳ Not received yet… (checked {} time(s))", attempt + 1)),
                }
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
            let b = found.ok_or("Build still not processed after 30 minutes — try again later.")?;
            push(format!("✅ Build ready ({})", b.id));

            if b.uses_non_exempt_encryption.is_none() {
                if !exempt {
                    return Err("Export compliance isn't answered for this build. Tick \"only exempt encryption\" if that's true, or answer it in App Store Connect.".into());
                }
                push("🔐 Declaring exempt encryption…".into());
                stores::asc_set_exempt_encryption(&jwt, &b.id).await?;
            }

            push(format!("🔗 Attaching build to version {version}…"));
            let version_id = asc_find_or_create_version(&jwt, &app_id, &version).await?;
            stores::asc_attach_build(&jwt, &version_id, &b.id).await?;

            push("📨 Submitting for review…".into());
            let submission = stores::asc_submit_for_review(&jwt, &app_id, &version_id).await?;
            push(format!("🎉 Submitted for review (submission {submission})"));
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                record_release(ws, "ios", &version, build, "Submitted for review");
                state.set(JobState::Ok(format!("{version} ({build}) submitted for review")));
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    });
}

pub(crate) fn release_aab(ws: Ws) {
    let mut state = ws.play_release_job;
    let mut log = ws.play_release_log;
    let dir = ws.dir();
    let p = ws.proj.read().clone();
    ws.show_job(JobKind::PlayRelease);
    let Some(aab) = ws.artifacts.read().aab.clone() else {
        state.set(JobState::Failed(
            "No AAB yet — build one in the Build step.".into(),
        ));
        return;
    };
    let track = p.play_track.clone();
    let status = p.play_release_status.clone();
    let version = p.version.trim().to_string();
    // Play caps release notes at 500 characters per language.
    let notes: Vec<(String, String)> = p
        .locales
        .iter()
        .filter_map(|l| {
            let text = p.store_texts.get(l)?.whats_new.trim().to_string();
            (!text.is_empty()).then(|| {
                (
                    stores::play_language(l).to_string(),
                    text.chars().take(500).collect(),
                )
            })
        })
        .collect();
    state.set(JobState::Running(format!("Releasing to {track}…")));
    log.set(Vec::new());

    ws.spawn(async move {
        let mut push = move |msg: String| log.write().push(msg);
        let result: Result<u32, String> = async {
            push("🔑 Authenticating with Google…".into());
            let (token, package) = play_auth(&dir, &p.android_bundle_id).await?;
            let edit_id = google_play_create_edit(&token, &package).await?;

            // Refuse early rather than after a long upload: the bundle's
            // versionCode must beat everything already on Play.
            let gradle = std::fs::read_to_string(
                dir.join(format!("target/dx/{}/release/android/app/app/build.gradle.kts", p.project_slug)),
            )
            .unwrap_or_default();
            if let Some(code) = verify::gradle_value(&gradle, "versionCode").and_then(|v| v.parse::<u32>().ok()) {
                if let Some((max, track)) = stores::play_max_version_code(&token, &package, &edit_id).await? {
                    if code <= max {
                        let _ = google_play_delete_edit(&token, &package, &edit_id).await;
                        return Err(format!(
                            "This AAB's versionCode {code} isn't higher than {max}, already on the {track} track — raise it in the Version step and rebuild."
                        ));
                    }
                }
            }

            let released: Result<u32, String> = async {
                push(format!("⬆  Uploading {}…", aab.summary()));
                let code = stores::play_upload_bundle(&token, &package, &edit_id, &aab.path).await?;
                push(format!("✅ Bundle accepted — versionCode {code}"));
                push(format!("🚦 Setting {track} track to {version} ({code}), status {status}…"));
                stores::play_update_track(&token, &package, &edit_id, stores::TrackRelease {
                    track: &track,
                    name: format!("{version} ({code})"),
                    version_code: code,
                    status: &status,
                    notes: &notes,
                })
                .await?;
                push("💾 Committing edit…".into());
                google_play_commit_edit(&token, &package, &edit_id).await?;
                Ok(code)
            }
            .await;
            if released.is_err() {
                let _ = google_play_delete_edit(&token, &package, &edit_id).await;
            }
            released
        }
        .await;

        match result {
            Ok(code) => {
                record_release(ws, "android", &version, code, &format!("Released to {track} ({status})"));
                state.set(JobState::Ok(format!("{version} ({code}) on {track}")));
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    });
}

/// Lines for the Store status card: ("App Store" | "Google Play", text).
pub(crate) fn fetch_store_status(
    ws: Ws,
    mut out: Signal<Vec<(String, String)>>,
    mut state: Signal<JobState>,
) {
    let dir = ws.dir();
    let p = ws.proj.read().clone();
    state.set(JobState::Running("Asking the stores…".into()));
    ws.spawn(async move {
        let mut lines = Vec::new();
        let mut errors = Vec::new();
        if p.platform_type.has_ios() {
            let r: Result<Vec<(String, String)>, String> = async {
                let jwt = asc_auth(&dir).await?;
                let app = asc_find_app(&jwt, p.ios_bundle_id.trim()).await?;
                stores::asc_app_versions(&jwt, &app).await
            }
            .await;
            match r {
                Ok(v) if v.is_empty() => lines.push(("App Store".into(), "No versions yet".into())),
                Ok(v) => lines.extend(
                    v.into_iter()
                        .map(|(ver, st)| ("App Store".to_string(), format!("{ver} — {st}"))),
                ),
                Err(e) => errors.push(format!("App Store: {e}")),
            }
        }
        if p.platform_type.has_android() {
            let r: Result<Vec<(String, String)>, String> = async {
                let (token, package) = play_auth(&dir, p.android_bundle_id.trim()).await?;
                let edit = google_play_create_edit(&token, &package).await?;
                let summary = stores::play_track_summary(&token, &package, &edit).await;
                let _ = google_play_delete_edit(&token, &package, &edit).await;
                summary
            }
            .await;
            match r {
                Ok(v) if v.is_empty() => {
                    lines.push(("Google Play".into(), "No releases on any track yet".into()))
                }
                Ok(v) => lines.extend(
                    v.into_iter()
                        .map(|(t, s)| ("Google Play".to_string(), format!("{t}: {s}"))),
                ),
                Err(e) => errors.push(format!("Google Play: {e}")),
            }
        }
        out.set(lines);
        state.set(if errors.is_empty() {
            JobState::Ok(format!("Updated {}", &utc_now()[11..]))
        } else {
            JobState::Failed(errors.join("\n"))
        });
    });
}
