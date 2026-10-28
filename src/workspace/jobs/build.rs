//! Building the app, and the signing it needs.

use super::*;

impl BuildInputs {
    pub(crate) fn of(dir: &std::path::Path, p: &ProjectState, s: &Settings) -> Self {
        Self {
            dir: dir.to_path_buf(),
            app_name: p.app_name.clone(),
            project_slug: p.project_slug.clone(),
            ios_bundle_id: p.ios_bundle_id.clone(),
            android_bundle_id: p.android_bundle_id.clone(),
            identity: s.apple_identity.clone(),
            profile: p.provisioning_profile.clone(),
            version: p.version.trim().to_string(),
            ios_build_number: p.ios_build_number,
            android_version_code: p.android_version_code,
            keystore_setting: s.android_keystore_path.clone(),
            alias: key_alias(p),
            logo_path: p.logo_path.clone(),
            logo_dest: p.logo_dest.clone(),
        }
    }

    /// Why `script_name` can't start, if it can't: checked before the job
    /// drawer shows the build as running.
    pub(crate) fn problem(&self, script_name: &str) -> Option<String> {
        if self.app_name.trim().is_empty()
            || self.project_slug.trim().is_empty()
            || (self.ios_bundle_id.trim().is_empty() && self.android_bundle_id.trim().is_empty())
        {
            return Some("App Name, Project Slug, and at least one Bundle ID are required. Fill them in the App step.".into());
        }
        if !valid_version(&self.version) {
            return Some("Set a valid version (e.g. 1.2.0) in the Version step.".into());
        }
        if script_name.contains("ios")
            && (self.identity.trim().is_empty() || self.profile.trim().is_empty())
        {
            return Some(
                "Apple Identity and Provisioning Profile are required for iOS. Set them in the Accounts step.".into(),
            );
        }
        // Resolved afresh: the project's .env may have changed since the card was drawn.
        if script_name == "build_android_release.sh"
            && !resolve_android_signing(&self.dir, &self.keystore_setting, &self.alias)
                .0
                .ok()
        {
            return Some(
                "The Android upload key isn't set up. Choose or create it in the Accounts step, under Android upload key."
                    .into(),
            );
        }
        None
    }
}

/// The build itself: the logo, the build scripts, the environment, then
/// ShipScreen's own Android build or the project's script. The window's
/// Build step and `mcp` both run this; the build numbers are bumped by the
/// caller, since each holds the project state its own way.
pub(crate) async fn build(i: BuildInputs, script_name: String, log: Log) -> Result<(), String> {
    let dir = i.dir.clone();
    // 0. Sync the logo to where this project's build looks for it, so the
    // build always uses the latest one. Always re-encode as genuine PNG — the
    // source may be a JPEG renamed to .png, which Apple's validator rejects
    // even though the extension looks right.
    let logo_dest = i.logo_dest.trim().to_string();
    if let (Some(src), Some(dest)) = (&i.logo_path, project_path(&dir, &logo_dest)) {
        let no_folder = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .is_some_and(|p| !p.is_dir());
        if no_folder {
            // A project that keeps its icon somewhere else entirely (an
            // asset catalogue, res/mipmap-*) simply has no folder of that
            // name: nothing to copy, and nothing wrong either.
            log.push(format!(
                "🖼️  Logo not copied — {logo_dest} isn't a folder in this project"
            ));
        } else {
            let src2 = src.clone();
            let dest2 = dest.clone();
            match tokio::task::spawn_blocking(move || {
                image::open(&src2)
                    .map_err(|e| e.to_string())
                    .and_then(|img| {
                        // Convert to RGB (drop alpha) and save as real PNG
                        img.to_rgb8()
                            .save_with_format(&dest2, image::ImageFormat::Png)
                            .map_err(|e| e.to_string())
                    })
            })
            .await
            {
                Ok(Ok(_)) => log.push(format!("🖼️  Logo saved as true PNG to {logo_dest}")),
                Ok(Err(e)) => log.push(format!("⚠️  Could not encode logo as PNG: {e}")),
                Err(e) => log.push(format!("⚠️  Logo task failed: {e}")),
            }
        }
    }

    // 1. Ensure scripts exist (create if missing)
    let created = tokio::task::spawn_blocking({
        let i = i.clone();
        move || {
            ensure_build_scripts(
                &i.dir,
                &i.app_name,
                &i.project_slug,
                &i.ios_bundle_id,
                &i.android_bundle_id,
                &i.identity,
                &i.profile,
                &i.version,
                i.ios_build_number,
                i.android_version_code,
            )
        }
    })
    .await
    .unwrap_or_default();

    for name in &created {
        log.push(format!("📝 Created {name}"));
    }
    // The script that will run: a project may bring one with a different
    // extension, and anything but a `.sh` is the developer's by definition.
    let script_path = build_script(&dir, &script_name);
    let own_script = script_path.as_ref().is_some_and(|p| {
        p.extension().is_some_and(|e| e != "sh") || !created.contains(&script_name)
    });
    if let Some(mine) = script_path.as_ref().filter(|_| own_script) {
        let name = mine.file_name().unwrap_or_default().to_string_lossy();
        log.push(format!(
            "📌 Using your own {name} as it is — ShipScreen only rewrites scripts it wrote. \
             The App, Version and Accounts steps reach it through the environment."
        ));
    }

    // 2. Everything the build needs from the environment: the project's
    //    .env, the upload key (team settings + password store) and Java.
    let env = load_env(&[dir.join(".env"), dir.join("fastlane").join(".env")]);
    let mut child_env: Vec<(String, String)> =
        env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    // What the generated scripts have baked in, handed to every script as the
    // environment: a script the developer wrote has no other way to reach the
    // Version and App steps. The project's own .env still wins.
    for (key, value) in [
        ("SHIPSCREEN", "1".to_string()),
        ("APP_NAME", i.app_name.clone()),
        ("PROJECT_SLUG", i.project_slug.clone()),
        ("VERSION", i.version.clone()),
        ("BUILD_NUMBER", i.ios_build_number.to_string()),
        ("VERSION_CODE", i.android_version_code.to_string()),
        ("IOS_BUNDLE_ID", i.ios_bundle_id.clone()),
        ("ANDROID_PACKAGE_NAME", i.android_bundle_id.clone()),
        ("SIGNING_IDENTITY", i.identity.clone()),
        ("PROVISIONING_PROFILE", i.profile.clone()),
    ] {
        if !value.trim().is_empty() && !env.contains_key(key) {
            child_env.push((key.to_string(), value));
        }
    }
    let (signing, password) = resolve_android_signing(&dir, &i.keystore_setting, &i.alias);
    if let (false, Some(ks)) = (
        env.contains_key("ANDROID_KEYSTORE_PATH"),
        signing.keystore.clone(),
    ) {
        child_env.push((
            "ANDROID_KEYSTORE_PATH".into(),
            ks.to_string_lossy().to_string(),
        ));
    }
    if !env.contains_key("ANDROID_KEY_ALIAS") {
        child_env.push(("ANDROID_KEY_ALIAS".into(), signing.alias.clone()));
    }
    if let (false, Some(pw)) = (env.contains_key("ANDROID_KEYSTORE_PASSWORD"), password) {
        child_env.push(("ANDROID_KEYSTORE_PASSWORD".into(), pw));
    }
    // Gradle needs a JDK it can run on: the one picked in the Java card,
    // else the project's JAVA_HOME, else the best installed — never one
    // too old or too new for the project's Gradle.
    let env_java = env
        .get("JAVA_HOME")
        .cloned()
        .or_else(|| std::env::var("JAVA_HOME").ok());
    let (jdk, notes) = if script_name.contains("ios") {
        (None, Vec::new())
    } else {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || java::for_project(&dir, env_java.as_deref()))
            .await
            .unwrap_or_default()
    };
    for n in notes {
        log.push(format!("⚠️  {n}"));
    }
    if let Some(j) = jdk {
        log.push(format!(
            "☕ Java {} ({}) · {}",
            j.version,
            j.source,
            j.home.display()
        ));
        let path = env
            .get("PATH")
            .map(Into::into)
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(j.home.join("bin")).chain(std::env::split_paths(&path)),
        )
        .unwrap_or(path);
        child_env.push(("JAVA_HOME".into(), j.home.to_string_lossy().to_string()));
        child_env.push(("PATH".into(), path.to_string_lossy().to_string()));
    }

    // 3. Android builds ShipScreen owns run in ShipScreen itself, the same
    //    on every OS. A script the developer wrote runs as it is.
    let dioxus = is_dioxus_project(&dir);
    match android_build::Kind::from_script(&script_name).filter(|_| !own_script && dioxus) {
        Some(kind) => {
            let find = |key: &str| {
                child_env
                    .iter()
                    .rev()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
            };
            let job = android_build::Job {
                kind,
                app_name: i.app_name.clone(),
                slug: i.project_slug.clone(),
                package: i.android_bundle_id.clone(),
                version_name: i.version.clone(),
                version_code: i.android_version_code,
                keystore: find("ANDROID_KEYSTORE_PATH").map(|v| signing::expand_home(&v)),
                alias: find("ANDROID_KEY_ALIAS").unwrap_or_default(),
                env: child_env.clone(),
            };
            android_build::run(dir.clone(), job, log).await.map(|_| ())
        }
        None => {
            let stem = script_name.strip_suffix(".sh").unwrap_or(&script_name);
            let Some(script_path) = script_path else {
                return Err(if dioxus {
                    format!("{script_name} not found in project directory")
                } else {
                    format!(
                        "{script_name} is not in the project folder. ShipScreen's own build \
                         scripts build Dioxus projects, so it writes none into this one — put \
                         your own {stem}.sh (or {stem}.cmd, or {stem}.ps1) there and it runs as \
                         it is, or build the app yourself and copy the .ipa or .aab into the \
                         project folder."
                    )
                });
            };
            if script_name.contains("ios") && !cfg!(target_os = "macos") {
                return Err("iOS builds need a Mac with Xcode.".into());
            }
            let ran = script_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut cmd = match script_path.extension().and_then(|e| e.to_str()) {
                Some("cmd" | "bat") => {
                    let mut cmd = std::process::Command::new("cmd");
                    cmd.arg("/C").arg(&script_path);
                    cmd
                }
                Some("ps1") => {
                    let mut cmd = if cfg!(windows) {
                        let mut cmd = std::process::Command::new("powershell");
                        cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
                        cmd
                    } else {
                        let mut cmd = std::process::Command::new("pwsh");
                        cmd.args(["-NoProfile", "-File"]);
                        cmd
                    };
                    cmd.arg(&script_path);
                    cmd
                }
                // A bash script, which is what ShipScreen writes itself.
                _ => {
                    let Some(bash) = find_bash() else {
                        return Err(format!(
                            "{ran} needs bash and this computer has none. Install Git for Windows \
                             (https://git-scm.com/download/win), put a {stem}.cmd or {stem}.ps1 \
                             next to it, or delete the script to let ShipScreen build Android itself."
                        ));
                    };
                    let mut cmd = std::process::Command::new(bash);
                    cmd.arg(&script_path);
                    cmd
                }
            };
            log.push(format!("▶ Running {ran}…"));
            cmd.current_dir(&dir).env("CI", "1");
            for (k, v) in &child_env {
                cmd.env(k, v);
            }
            stream_command(cmd, &ran, log).await
        }
    }
}

pub(crate) fn run_build(ws: Ws, script_name: String) {
    let mut build_phase = ws.build_phase;
    let mut build_log = ws.build_log;
    let mut refresh = ws.refresh;
    let dir = ws.dir();
    let inputs = BuildInputs::of(&dir, &ws.proj.read(), &ws.settings.read());
    if let Some(problem) = inputs.problem(&script_name) {
        build_phase.set(BuildPhase::Error(problem));
        ws.show_job(JobKind::Build);
        return;
    }

    build_phase.set(BuildPhase::Running(script_name.clone()));
    build_log.set(Vec::new());
    let mut last_build = ws.last_build;
    last_build.set(script_name.clone());
    ws.show_job(JobKind::Build);

    ws.spawn(async move {
        match build(inputs, script_name.clone(), build_log.into()).await {
            Ok(()) => {
                build_phase.set(BuildPhase::Success(script_name.clone()));
                ws.update(|p| bump_after_build(p, &dir, &script_name));
            }
            Err(e) => build_phase.set(BuildPhase::Error(e)),
        }
        refresh.with_mut(|n| *n += 1);
    });
}

/// bash for a developer's own build script: on Windows, Git for Windows'.
pub(crate) fn find_bash() -> Option<PathBuf> {
    if !cfg!(windows) {
        return Some(PathBuf::from("bash"));
    }
    ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .flat_map(|base| {
            let base = PathBuf::from(base);
            [
                base.join("Git").join("bin").join("bash.exe"),
                base.join("Programs")
                    .join("Git")
                    .join("bin")
                    .join("bash.exe"),
            ]
        })
        .find(|p| p.is_file())
}

/// Run a command on a plain OS thread (Signals are !Send), streaming its
/// stdout and stderr into `log` line by line. Lines flow back over mpsc and are
/// pushed from this task, on the UI thread, so Dioxus re-renders live.
pub(crate) async fn stream_command(
    mut cmd: std::process::Command,
    name: &str,
    log: impl Into<Log>,
) -> Result<(), String> {
    let log = log.into();
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::sync::mpsc;

    enum Msg {
        Line(String),
        Done(Result<(), String>),
    }
    let (tx, rx) = mpsc::channel::<Msg>();
    let name_for_thread = name.to_string();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_console(&mut cmd);

    std::thread::spawn(move || {
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Msg::Done(Err(format!(
                    "Could not start {name_for_thread}: {e}"
                ))));
                return;
            }
        };
        let tx2 = tx.clone();
        let stderr_stream = child.stderr.take();
        std::thread::spawn(move || {
            if let Some(r) = stderr_stream.map(BufReader::new) {
                for line in r.lines().map_while(Result::ok) {
                    let _ = tx2.send(Msg::Line(line));
                }
            }
        });
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(Msg::Line(line));
            }
        }
        let result = match child.wait() {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!(
                "{name_for_thread} exited with code {}",
                s.code().unwrap_or(-1)
            )),
            Err(e) => Err(format!("wait: {e}")),
        };
        let _ = tx.send(Msg::Done(result));
    });

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        for msg in rx.try_iter() {
            match msg {
                Msg::Line(line) => {
                    let clean = strip_ansi(&line);
                    if !clean.trim().is_empty() {
                        log.push(clean);
                    }
                }
                Msg::Done(result) => return result,
            }
        }
    }
}

/// New key + CSR → Apple Distribution certificate → .p12 in the keys folder
/// and the login keychain, then select it as the signing identity.
pub(crate) fn create_distribution_certificate(
    ws: Ws,
    name: String,
    email: String,
    p12_password: String,
) {
    let mut state = ws.cert_job;
    let mut settings = ws.settings;
    let dir = ws.dir();
    let keys = keys_dir(&settings.read());
    state.set(JobState::Running("Creating certificate…".into()));
    ws.spawn(async move {
        let result: Result<String, String> = async {
            let jwt = asc_auth(&dir).await?;
            let stem = format!("apple_distribution_{}", utc_now()[..10].replace('-', ""));
            let (key, csr) = {
                let (keys, stem, email, name) = (keys.clone(), stem.clone(), email.clone(), name.clone());
                tokio::task::spawn_blocking(move || signing::apple_key_and_csr(&keys, &stem, &email, &name))
                    .await
                    .map_err(|e| e.to_string())??
            };
            let (_, der) = stores::asc_create_distribution_certificate(&jwt, &csr).await.map_err(|e| {
                format!("{e}\nCreating certificates needs an API key with enough access (Admin) — or create it in the developer portal.")
            })?;
            let label = format!("Apple Distribution: {name}");
            let der2 = der.clone();
            tokio::task::spawn_blocking(move || signing::import_apple_identity(&key, &der2, &label, &p12_password))
                .await
                .map_err(|e| e.to_string())??;
            // The keychain's own name for it, e.g. "Apple Distribution: Jane (TEAMID)".
            let sha1 = signing::cert_sha1(&der).unwrap_or_default();
            let identity = tokio::task::spawn_blocking(move || {
                std::process::Command::new("security")
                    .args(["find-identity", "-v", "-p", "codesigning"])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .and_then(|out| {
                        out.lines()
                            .find(|l| l.contains(&sha1))
                            .and_then(|l| l.split('"').nth(1).map(str::to_string))
                    })
            })
            .await
            .ok()
            .flatten()
            .ok_or("Certificate created, but it doesn't show up as a signing identity — check Keychain Access.")?;
            Ok(identity)
        }
        .await;
        match result {
            Ok(identity) => {
                settings.write().apple_identity = identity.clone();
                save_settings(&settings.read());
                state.set(JobState::Ok(format!("{identity} — installed, with a .p12 backup in the keys folder")));
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    });
}

/// Replace this app's App Store profile with a fresh one on a distribution
/// certificate whose private key is on this Mac, install it, and select it.
pub(crate) fn repair_profile(ws: Ws) {
    let mut state = ws.profile_job;
    let mut refresh = ws.refresh;
    let dir = ws.dir();
    let bundle = ws.proj.read().ios_bundle_id.trim().to_string();
    let app_name = ws.proj.read().app_name.trim().to_string();
    if bundle.is_empty() {
        state.set(JobState::Failed(
            "Set the iOS bundle ID in the App step first.".into(),
        ));
        return;
    }
    state.set(JobState::Running("Regenerating profile…".into()));
    ws.spawn(async move {
        let result: Result<String, String> = async {
            let jwt = asc_auth(&dir).await?;
            let local = tokio::task::spawn_blocking(signing::keychain_identity_hashes).await.unwrap_or_default();
            let mut certs: Vec<_> = stores::asc_distribution_certificates(&jwt)
                .await?
                .into_iter()
                .filter(|c| signing::cert_sha1(&c.der).is_some_and(|h| local.contains(&h)))
                .collect();
            certs.sort_by(|a, b| b.expires.cmp(&a.expires));
            let cert = certs.first().ok_or(
                "No distribution certificate with its private key on this Mac — create one above first.",
            )?;
            let bundle_res = stores::asc_bundle_id(&jwt, &bundle).await?;
            let name = format!("{} App Store", if app_name.is_empty() { &bundle } else { &app_name });
            for old in stores::asc_bundle_profiles(&jwt, &bundle_res).await? {
                if old.profile_type == "IOS_APP_STORE" && (old.state == "INVALID" || old.name == name) {
                    stores::asc_delete_profile(&jwt, &old.id).await?;
                }
            }
            let content = stores::asc_create_app_store_profile(&jwt, &name, &bundle_res, &cert.id).await?;
            let path = checks::install_profile(&content)?;
            Ok(path.to_string_lossy().to_string())
        }
        .await;
        match result {
            Ok(path) => {
                ws.update(|p| p.provisioning_profile = path.clone());
                refresh.with_mut(|n| *n += 1);
                state.set(JobState::Ok("New App Store profile installed and selected".into()));
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    });
}
