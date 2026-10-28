//! Signing settings, credentials and the built files a project has.

use super::*;

pub(super) fn read_creds(dir: &std::path::Path) -> Creds {
    let resolve = env_lookup(dir);
    let value = |label: &'static str, key: &str| match resolve(key) {
        Some(_) => CredCheck {
            label,
            ok: true,
            detail: format!("{key} is set"),
        },
        None => CredCheck {
            label,
            ok: false,
            detail: format!("{key} is not set"),
        },
    };
    // `~/…` as the build script expands it
    let expand = |path: String| match path.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|h| h.join(rest).to_string_lossy().to_string())
            .unwrap_or(path),
        None => path,
    };
    let file = |label: &'static str, key: &str| match resolve(key).map(expand) {
        Some(path) if std::path::Path::new(&path).is_file() => CredCheck {
            label,
            ok: true,
            detail: path,
        },
        Some(path) => CredCheck {
            label,
            ok: false,
            detail: format!("File not found: {path}"),
        },
        None => CredCheck {
            label,
            ok: false,
            detail: format!("{key} is not set"),
        },
    };
    Creds {
        app_store: vec![
            value("Key ID", "APP_STORE_CONNECT_API_KEY_KEY_ID"),
            value("Issuer ID", "APP_STORE_CONNECT_API_KEY_ISSUER_ID"),
            file(
                "Private key (.p8)",
                "APP_STORE_CONNECT_API_KEY_KEY_FILEPATH",
            ),
        ],
        play: vec![file("Service-account JSON", "GOOGLE_PLAY_JSON_KEY")],
    }
}

impl AndroidSigning {
    pub(super) fn checks(&self) -> Vec<CredCheck> {
        let keystore = match &self.keystore {
            Some(p) if p.is_file() => CredCheck {
                label: "Upload keystore",
                ok: true,
                detail: format!(
                    "{}{}",
                    p.display(),
                    if self.keystore_from_env {
                        " (from .env)"
                    } else {
                        ""
                    }
                ),
            },
            Some(p) => CredCheck {
                label: "Upload keystore",
                ok: false,
                detail: format!("File not found: {}", p.display()),
            },
            None => CredCheck {
                label: "Upload keystore",
                ok: false,
                detail: "Not set — choose or create one below".into(),
            },
        };
        let password = CredCheck {
            label: "Keystore password",
            ok: self.password.is_some(),
            detail: match self.password {
                Some(PasswordSource::Keychain) => format!("In {}", signing::password_store_name()),
                Some(PasswordSource::Env) if signing::has_password_store() => format!(
                    "In the project's .env — {} is safer",
                    signing::password_store_name()
                ),
                Some(PasswordSource::Env) => "In the project's .env".into(),
                None => "Not saved yet".into(),
            },
        };
        vec![keystore, password]
    }

    pub(super) fn ok(&self) -> bool {
        self.checks().iter().all(|c| c.ok)
    }
}

/// Resolve the upload key for a project, returning the password separately
/// so it never sits in UI state.
pub(super) fn resolve_android_signing(
    dir: &std::path::Path,
    keystore_setting: &str,
    alias: &str,
) -> (AndroidSigning, Option<String>) {
    let resolve = env_lookup(dir);
    let from_env = resolve("ANDROID_KEYSTORE_PATH");
    let keystore = from_env
        .clone()
        .or_else(|| {
            (!keystore_setting.trim().is_empty()).then(|| keystore_setting.trim().to_string())
        })
        .map(|p| signing::expand_home(&p));
    let alias = resolve("ANDROID_KEY_ALIAS").unwrap_or_else(|| alias.to_string());
    let (password, source) = match resolve("ANDROID_KEYSTORE_PASSWORD") {
        Some(p) => (Some(p), Some(PasswordSource::Env)),
        None => match keystore
            .as_ref()
            .and_then(|k| signing::keychain_get(&signing::keystore_account(k)))
        {
            Some(p) => (Some(p), Some(PasswordSource::Keychain)),
            None => (None, None),
        },
    };
    (
        AndroidSigning {
            keystore,
            keystore_from_env: from_env.is_some(),
            alias,
            password: source,
        },
        password,
    )
}

impl Artifacts {
    /// Keep `path` if it's the newest file of its kind seen so far.
    pub(super) fn consider(&mut self, path: std::path::PathBuf) {
        let Ok(meta) = std::fs::metadata(&path) else {
            return;
        };
        if !meta.is_file() {
            return;
        }
        let slot = match path.extension().and_then(|e| e.to_str()) {
            Some("ipa") => &mut self.ipa,
            Some("aab") => &mut self.aab,
            Some("apk") => &mut self.apk,
            _ => return,
        };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if slot.as_ref().is_none_or(|a| modified > a.modified) {
            *slot = Some(Artifact {
                path,
                modified,
                size: meta.len(),
            });
        }
    }

    pub(super) fn consider_dir(&mut self, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            self.consider(entry.path());
        }
    }
}

/// Where a built bundle may be: the project folder, where the build scripts
/// copy their final bundle (`<App>.ipa`, `<slug>_release.aab`,
/// `<slug>-debug.apk`), plus the paths the Build step names for a project that
/// builds itself — Xcode and Gradle leave theirs deep inside a build folder.
/// The newest file of each kind is the one that counts.
pub(super) fn scan_artifacts(
    dir: &std::path::Path,
    ios_path: &str,
    android_path: &str,
) -> Artifacts {
    let mut found = Artifacts::default();
    found.consider_dir(dir);
    for path in [ios_path, android_path]
        .into_iter()
        .filter_map(|p| project_path(dir, p))
    {
        if path.is_dir() {
            found.consider_dir(&path);
        } else {
            found.consider(path);
        }
    }
    found
}
