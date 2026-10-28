//! ShipScreen's own settings, and the keys it reads from the environment.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Settings {
    pub(super) fal_key: String,
    #[serde(default = "default_phone_style")]
    pub(super) phone_style: String,
    pub(super) inference_steps: u32,
    #[serde(default)]
    pub(super) recent_projects: Vec<PathBuf>,
    // Build / signing config (same developer across all projects)
    #[serde(default)]
    pub(super) apple_identity: String, // "Apple Distribution: Name (TEAMID)"
    #[serde(default)]
    pub(super) provisioning_profile: String, // absolute path to .mobileprovision
    #[serde(default = "default_ios_short_version")]
    pub(super) ios_short_version: String, // e.g. "1.0"
    // Signing material shared by every project (one developer, one team)
    /// Android upload keystore; its password lives in the Keychain
    #[serde(default)]
    pub(super) android_keystore_path: String,
    /// Folder holding keys and certificates, outside every project ("" = ~/keys)
    #[serde(default)]
    pub(super) keys_dir: String,
    /// When the last encrypted backup of the keys folder was written (UTC)
    #[serde(default)]
    pub(super) last_backup_at: String,
    /// Where it was written
    #[serde(default)]
    pub(super) last_backup_path: String,
    /// JDK home ShipScreen runs keytool and Gradle with ("" = recommended)
    #[serde(default)]
    pub(super) java_home: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fal_key: std::env::var("FAL_KEY").unwrap_or_default(),
            phone_style: default_phone_style(),
            inference_steps: 28,
            recent_projects: Vec::new(),
            apple_identity: String::new(),
            provisioning_profile: String::new(),
            ios_short_version: "1.0".to_string(),
            android_keystore_path: String::new(),
            keys_dir: String::new(),
            last_backup_at: String::new(),
            last_backup_path: String::new(),
            java_home: String::new(),
        }
    }
}

/// Load key=value pairs from a .env file into a map.
pub(super) fn parse_dotenv(path: &PathBuf) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(contents) = std::fs::read_to_string(path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                map.insert(k.trim().to_string(), v.to_string());
            }
        }
    }
    map
}

/// Merge env vars from multiple .env files and the process environment.
/// Later sources win (project .env > app .env > process env).
/// Returns a map and a resolver closure isn't possible here, so callers use
/// the returned map + std::env::var fallback.
pub(super) fn load_env(dotenv_paths: &[PathBuf]) -> std::collections::HashMap<String, String> {
    let mut merged = std::collections::HashMap::new();

    // Collect candidate directories to look for an ShipScreen-level .env:
    //   1. current working directory  (always correct for `cargo run`)
    //   2. next to the executable     (correct for an installed/release binary)
    //   3. walk up from exe until we find a .env (handles target/debug/ nesting)
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        search_dirs.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        // Walk up from the exe dir looking for a .env
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            search_dirs.push(d.clone());
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    for dir in &search_dirs {
        let candidate = dir.join(".env");
        if candidate.exists() {
            for (k, v) in parse_dotenv(&candidate) {
                merged.entry(k).or_insert(v); // first match wins (cwd beats target/debug)
            }
            break; // stop at the first .env found walking up
        }
    }

    // Then overlay each caller-supplied path (project-level, later = higher priority)
    for path in dotenv_paths {
        for (k, v) in parse_dotenv(path) {
            merged.insert(k, v);
        }
    }
    merged
}

#[cfg(test)]
mod tests;
