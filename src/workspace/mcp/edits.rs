//! What the tools read and change in a project.

use super::*;

impl Project {
    pub(super) fn from(args: &Value) -> Result<Self, String> {
        let dir = PathBuf::from(text(args, "project")?);
        if !dir.is_absolute() {
            return Err("project must be an absolute path".into());
        }
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        let known = load_settings().recent_projects.iter().any(|p| p == &dir);
        if !known
            && !dir.join("shipscreen.json").is_file()
            && !dir.join("appscreens.json").is_file()
        {
            return Err(format!(
                "{} is not a ShipScreen project: open it once in ShipScreen, or pick one from list_projects",
                dir.display()
            ));
        }
        Ok(Self { dir })
    }

    /// The state as on disk now, seeded from the project's own files the way
    /// the window seeds it when it opens the project.
    pub(super) fn state(&self) -> ProjectState {
        let mut state = load_project_state(&self.dir);
        let identified = seed_identity(&mut state, &self.dir);
        if seed_release_fields(&mut state, &self.dir, &load_settings()) | identified {
            save_project_state(&self.dir, &state);
        }
        state
    }

    /// Applies `f` to the state as on disk now and saves it.
    pub(super) fn change<R>(
        &self,
        f: impl FnOnce(&mut ProjectState) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut state = self.state();
        let r = f(&mut state)?;
        save_project_state(&self.dir, &state);
        Ok(r)
    }

    pub(super) fn describe(&self) -> Value {
        describe(&self.dir, &self.state(), &load_settings())
    }
}

/// `describe` as a client reads it: each language's store text as how full
/// each field is, and the words themselves only for `language`. Every store
/// text in every language is most of a project, and a client asking about
/// its version or a build doesn't need it.
pub(super) fn brief(mut project: Value, language: Option<&str>) -> Value {
    if let Some(store) = project["store_text"].as_object_mut() {
        for (locale, fields) in store.iter_mut() {
            if Some(locale.as_str()) == language {
                continue;
            }
            for field in fields
                .as_object_mut()
                .into_iter()
                .flat_map(|f| f.values_mut())
            {
                if let Some(f) = field.as_object_mut() {
                    f.remove("text");
                }
            }
        }
    }
    project
}

pub(super) fn describe(dir: &std::path::Path, p: &ProjectState, settings: &Settings) -> Value {
    let languages: Vec<Value> = p
        .locales
        .iter()
        .map(|c| json!({ "code": c, "name": locale_name(c) }))
        .collect();
    let mut screens = serde_json::Map::new();
    let mut store = serde_json::Map::new();
    for locale in &p.locales {
        let texts = p.texts_for(locale);
        let list: Vec<Value> = p
            .sources_for(locale)
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let (title, subtitle) = texts.get(i).cloned().unwrap_or_default();
                json!({ "screen": i + 1, "path": path, "exists": path.is_file(), "title": title, "subtitle": subtitle })
            })
            .collect();
        screens.insert(locale.clone(), Value::Array(list));
        let t = p.store_texts.get(locale).cloned().unwrap_or_default();
        let mut fields = serde_json::Map::new();
        for (field, limit) in STORE_FIELDS {
            let value = store_field(&t, field).to_string();
            fields.insert(
                field.into(),
                json!({ "text": value, "chars": value.chars().count(), "limit": limit }),
            );
        }
        store.insert(locale.clone(), Value::Object(fields));
    }

    let artifact = |a: &Option<Artifact>| {
        a.as_ref()
            .map(|a| json!({ "path": a.path, "made": ago(a.modified) }))
    };
    let found = scan_artifacts(dir, &p.ios_artifact_path, &p.android_artifact_path);
    let creds = read_creds(dir);
    let inputs = jobs::BuildInputs::of(dir, p, settings);
    let mut builds = serde_json::Map::new();
    for (target, script, applies) in [
        (
            "ios",
            "build_ios_distribution.sh",
            p.platform_type.has_ios(),
        ),
        (
            "aab",
            "build_android_release.sh",
            p.platform_type.has_android(),
        ),
        ("apk", "build_apk.sh", p.platform_type.has_android()),
    ] {
        if applies {
            builds.insert(
                target.into(),
                json!(inputs.problem(script).unwrap_or_else(|| "ready".into())),
            );
        }
    }
    let checks = |list: &[CredCheck]| -> Vec<Value> {
        list.iter()
            .map(|c| json!({ "check": c.label, "ok": c.ok, "detail": c.detail }))
            .collect()
    };
    let (signing, _) = resolve_android_signing(dir, &settings.android_keystore_path, &key_alias(p));

    json!({
        "project": dir,
        "app_name": p.app_name,
        "project_slug": p.project_slug,
        "platform": p.platform_type.label(),
        "ios_bundle_id": p.ios_bundle_id,
        "android_package": p.android_bundle_id,
        "support_url": p.support_url,
        "primary_color": p.primary_color,
        "secondary_color": p.secondary_color,
        "exempt_encryption": p.exempt_encryption,
        "play_track": p.play_track,
        "play_release_status": p.play_release_status,
        "version": p.version,
        "ios_build_number": p.ios_build_number,
        "android_version_code": p.android_version_code,
        "auto_increment_build": p.auto_increment_build,
        "languages": languages,
        "screens": screens,
        "store_text": store,
        "screenshots_generated": p.output_paths.len(),
        "builds_made": { "ipa": artifact(&found.ipa), "aab": artifact(&found.aab), "apk": artifact(&found.apk) },
        "build_readiness": builds,
        "credentials": {
            "app_store": checks(&creds.app_store),
            "google_play": checks(&creds.play),
            "android_upload_key": checks(&signing.checks()),
            "apple_identity": settings.apple_identity,
            "provisioning_profile": p.provisioning_profile,
        },
        "releases": p.releases.iter().take(10).map(|r| json!({
            "at": r.at, "platform": r.platform, "version": r.version, "build": r.build, "action": r.action,
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn set_app(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    project.change(|p| {
        let s = |key: &str| args[key].as_str().map(|v| v.trim().to_string());
        if let Some(v) = s("app_name") {
            p.app_name = v;
        }
        if let Some(v) = s("project_slug") {
            p.project_slug = v;
        }
        if let Some(v) = s("ios_bundle_id") {
            p.ios_bundle_id = v;
        }
        if let Some(v) = s("android_package") {
            p.android_bundle_id = v;
        }
        if let Some(v) = s("support_url") {
            p.support_url = v;
        }
        for (key, slot) in [
            ("primary_color", &mut p.primary_color),
            ("secondary_color", &mut p.secondary_color),
        ] {
            if let Some(v) = s(key) {
                if parse_hex_color(&v).is_none() {
                    return Err(format!("{key} is a colour like #3B82F6, not {v}"));
                }
                *slot = v;
            }
        }
        if let Some(v) = args["exempt_encryption"].as_bool() {
            p.exempt_encryption = v;
        }
        if let Some(v) = s("play_track") {
            if !["internal", "alpha", "beta", "production"].contains(&v.as_str()) {
                return Err(format!(
                    "play_track is internal, alpha, beta or production, not {v}"
                ));
            }
            p.play_track = v;
        }
        if let Some(v) = s("play_release_status") {
            if !["draft", "completed"].contains(&v.as_str()) {
                return Err(format!(
                    "play_release_status is draft or completed, not {v}"
                ));
            }
            p.play_release_status = v;
        }
        Ok(())
    })?;
    Ok(brief(project.describe(), None))
}

pub(super) fn set_version(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let v = project.change(|p| {
        if let Some(v) = args["version"].as_str().map(str::trim) {
            if !valid_version(v) {
                return Err(format!(
                    "{v} is not a version the stores take; use numbers like 1.2.0"
                ));
            }
            p.version = v.to_string();
        }
        if let Some(b) = args["bump"].as_str() {
            let idx = match b {
                "major" => 0,
                "minor" => 1,
                "patch" => 2,
                other => return Err(format!("bump is major, minor or patch, not {other}")),
            };
            p.version = bump_version(&p.version, idx);
        }
        let number = |key: &str| {
            args[key]
                .as_u64()
                .map(|n| u32::try_from(n).unwrap_or(u32::MAX).max(1))
        };
        if let Some(n) = number("ios_build_number") {
            p.ios_build_number = n;
        }
        if let Some(n) = number("android_version_code") {
            p.android_version_code = n;
        }
        if let Some(on) = args["auto_increment_build"].as_bool() {
            p.auto_increment_build = on;
        }
        Ok(json!({
            "version": p.version,
            "ios_build_number": p.ios_build_number,
            "android_version_code": p.android_version_code,
            "auto_increment_build": p.auto_increment_build,
        }))
    })?;
    Ok(v)
}

pub(super) fn set_languages(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let list = |key: &str| -> Result<Vec<&'static str>, String> {
        args[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| known_locale(v.as_str().unwrap_or("")))
            .collect()
    };
    let (add, remove) = (list("add")?, list("remove")?);
    if remove.contains(&"en-US") {
        return Err(
            "English (en-US) is always kept: it is the default language in both stores".into(),
        );
    }
    project.change(|p| {
        p.locales.retain(|l| !remove.contains(&l.as_str()));
        for code in add {
            if !p.locales.iter().any(|l| l == code) {
                p.locales.push(code.to_string());
                p.locale_sources.entry(code.to_string()).or_default();
                p.ensure_texts_len(code, 0);
            }
        }
        Ok(())
    })?;
    Ok(brief(project.describe(), None))
}

pub(super) fn set_screens(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let locale = known_locale(&text(args, "locale")?)?;
    project.change(|p| {
        if !p.locales.iter().any(|l| l == locale) {
            return Err(format!(
                "{locale} is not one of the project's languages: add it with set_languages"
            ));
        }
        let mut sources = p.sources_for(locale);
        let mut texts = p.texts_for(locale);
        texts.resize(
            sources.len().max(texts.len()),
            (String::new(), String::new()),
        );
        let count = sources.len();
        let screen = |n: u64| -> Result<usize, String> {
            (n as usize)
                .checked_sub(1)
                .filter(|i| *i < count)
                .ok_or(format!("no screen {n}: {locale} has {count}"))
        };
        for c in args["captions"].as_array().into_iter().flatten() {
            let i = screen(
                c["screen"]
                    .as_u64()
                    .ok_or("each caption needs its screen number")?,
            )?;
            if let Some(t) = c["title"].as_str() {
                texts[i].0 = t.trim().to_string();
            }
            if let Some(t) = c["subtitle"].as_str() {
                texts[i].1 = t.trim().to_string();
            }
        }
        let mut gone: Vec<usize> = args["remove"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| screen(v.as_u64().unwrap_or(0)))
            .collect::<Result<_, _>>()?;
        gone.sort_unstable();
        gone.dedup();
        for i in gone.into_iter().rev() {
            sources.remove(i);
            if i < texts.len() {
                texts.remove(i);
            }
        }
        texts.truncate(sources.len());
        for a in args["add"].as_array().into_iter().flatten() {
            let path = PathBuf::from(
                a["path"]
                    .as_str()
                    .ok_or("each screen added needs its path")?,
            );
            let image = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["png", "jpg", "jpeg"].contains(&e.to_ascii_lowercase().as_str()));
            if !path.is_absolute() || !path.is_file() || !image {
                return Err(format!(
                    "{} is not a PNG or JPEG file (an absolute path)",
                    path.display()
                ));
            }
            if sources.contains(&path) {
                continue;
            }
            sources.push(path);
            let caption = |key: &str| a[key].as_str().unwrap_or("").trim().to_string();
            texts.push((caption("title"), caption("subtitle")));
        }
        p.locale_sources.insert(locale.to_string(), sources);
        p.locale_texts.insert(locale.to_string(), texts);
        Ok(())
    })?;
    Ok(project.describe()["screens"][locale].clone())
}

pub(super) fn set_store_text(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let locale = known_locale(&text(args, "locale")?)?;
    for (field, limit) in STORE_FIELDS {
        if let Some(v) = args[field].as_str() {
            let n = v.trim().chars().count();
            if n > limit {
                return Err(format!("{field} is {n} characters; the stores take at most {limit}. Nothing was saved."));
            }
        }
    }
    project.change(|p| {
        if !p.locales.iter().any(|l| l == locale) {
            return Err(format!(
                "{locale} is not one of the project's languages: add it with set_languages"
            ));
        }
        let t = p.store_texts.entry(locale.to_string()).or_default();
        for (field, _) in STORE_FIELDS {
            if let Some(v) = args[field].as_str() {
                *store_field_mut(t, field) = v.trim().to_string();
            }
        }
        Ok(())
    })?;
    Ok(project.describe()["store_text"][locale].clone())
}

pub(super) async fn generate(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let state = project.state();
    let (log, collected) = lines();
    let mut previews = Vec::new();
    let outputs = jobs::generate_manual_images(project.dir.clone(), state, log, |enc| {
        previews.push(format!("/localimg/{enc}"))
    })
    .await?;
    let count = outputs.len();
    project.change(|p| {
        p.generated_urls = previews;
        p.output_paths = outputs;
        Ok(())
    })?;
    let folders: std::collections::BTreeSet<String> = project
        .state()
        .output_paths
        .iter()
        .filter_map(|(_, path)| path.parent().map(|d| d.display().to_string()))
        .collect();
    let log = collected.borrow().clone();
    Ok(json!({ "files": count, "folders": folders, "log": tail(&log) }))
}

pub(super) async fn run(args: &Value) -> Result<Value, String> {
    let project = Project::from(args)?;
    let script = match text(args, "target")?.as_str() {
        "ios" => "build_ios_distribution.sh",
        "aab" => "build_android_release.sh",
        "apk" => "build_apk.sh",
        other => return Err(format!("target is ios, aab or apk, not {other}")),
    };
    let inputs = jobs::BuildInputs::of(&project.dir, &project.state(), &load_settings());
    if let Some(problem) = inputs.problem(script) {
        return Err(problem);
    }
    let (log, collected) = lines();
    let outcome = jobs::build(inputs, script.to_string(), log).await;
    let log = collected.borrow().clone();
    match outcome {
        Ok(()) => {
            let numbers = project.change(|p| {
                jobs::bump_after_build(p, &project.dir, script);
                Ok(json!({ "next_ios_build_number": p.ios_build_number, "next_android_version_code": p.android_version_code }))
            })?;
            let state = project.state();
            let found = scan_artifacts(
                &project.dir,
                &state.ios_artifact_path,
                &state.android_artifact_path,
            );
            let made = match script {
                "build_ios_distribution.sh" => found.ipa,
                "build_android_release.sh" => found.aab,
                _ => found.apk,
            };
            Ok(json!({ "built": made.map(|a| a.path), "numbers": numbers, "log": tail(&log) }))
        }
        Err(e) => Err(format!(
            "{e}\n\nLast lines of the build:\n{}",
            tail(&log).join("\n")
        )),
    }
}
