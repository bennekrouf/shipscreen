//! What ShipScreen keeps about a project, in the project's own folder.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub(super) struct ProjectState {
    /// Legacy shared source paths (kept for backward compat; migrated into locale_sources on load)
    #[serde(default)]
    pub(super) source_paths: Vec<PathBuf>,
    /// Per-locale source images: locale code → ordered list of image paths
    #[serde(default)]
    pub(super) locale_sources: std::collections::HashMap<String, Vec<PathBuf>>,
    /// Legacy single-locale texts (kept for backward compat; migrated into locale_texts on load)
    #[serde(default)]
    pub(super) manual_texts: Vec<(String, String)>, // (title, subtitle) per image
    /// Per-locale texts: locale code → vec of (title, subtitle) per source image
    #[serde(default)]
    pub(super) locale_texts: std::collections::HashMap<String, Vec<(String, String)>>,
    #[serde(default)]
    pub(super) primary_color: String,
    #[serde(default)]
    pub(super) secondary_color: String,
    #[serde(default)]
    pub(super) theme_prompt: String,
    #[serde(default)]
    pub(super) theme_history: Vec<String>,
    /// Legacy single locale (kept for backward compat; use `locales` instead)
    #[serde(default = "default_locale")]
    pub(super) locale: String,
    /// Currently selected locales for generation/publishing
    #[serde(default = "default_locales")]
    pub(super) locales: Vec<String>,
    #[serde(default)]
    pub(super) generated_urls: Vec<String>,
    #[serde(default)]
    pub(super) output_paths: Vec<(String, PathBuf)>,
    #[serde(default)]
    pub(super) logo_path: Option<PathBuf>,
    /// Where the logo is written before each build, relative to the project
    /// (or absolute). `assets/icon.png` is where a Dioxus build looks for it;
    /// empty means the logo isn't copied into the project at all.
    #[serde(default = "default_logo_dest")]
    pub(super) logo_dest: String,
    // Build configuration (per-project)
    #[serde(default)]
    pub(super) app_name: String, // Display name, e.g. "Abjad"
    #[serde(default)]
    pub(super) project_slug: String, // Lowercase dx slug, e.g. "abjad"
    /// Legacy single bundle ID field — kept for deserialization of old JSON only.
    /// On load this is migrated into ios_bundle_id / android_bundle_id and then ignored.
    #[serde(default, skip_serializing)]
    pub(super) bundle_id: String,
    #[serde(default)]
    pub(super) ios_bundle_id: String, // iOS bundle ID, e.g. "com.mayorana.tafseel.mufrad"
    #[serde(default)]
    pub(super) android_bundle_id: String, // Android package, e.g. "com.mayorana.mufrad"
    #[serde(default)]
    pub(super) platform_type: PlatformType, // Target platform(s) for this project
    // Export configuration
    #[serde(default = "default_true")]
    pub(super) export_ios: bool,
    #[serde(default = "default_true")]
    pub(super) export_android: bool,
    /// One bool per IOS_TARGETS entry (all enabled by default)
    #[serde(default = "default_ios_targets")]
    pub(super) ios_targets: Vec<bool>,
    /// One bool per ANDROID_TARGETS entry (all enabled by default)
    #[serde(default = "default_android_targets")]
    pub(super) android_targets: Vec<bool>,
    /// One bool per DESKTOP_TARGETS entry (all enabled by default)
    #[serde(default = "default_desktop_targets")]
    pub(super) desktop_targets: Vec<bool>,
    // Release numbering (per project). Empty/0 means "not seeded yet" — the
    // workspace seeds them on open from the old sources (global setting,
    // build_number.txt, Dioxus.toml) so numbers never go backwards.
    /// Marketing version, e.g. "1.2.0" (CFBundleShortVersionString / versionName)
    #[serde(default)]
    pub(super) version: String,
    /// Build number the next iOS build will use (CFBundleVersion)
    #[serde(default)]
    pub(super) ios_build_number: u32,
    /// versionCode the next Android release build will use
    #[serde(default)]
    pub(super) android_version_code: u32,
    /// Bump the build number after each successful release build
    #[serde(default = "default_true")]
    pub(super) auto_increment_build: bool,
    /// Provisioning profile for this app (profiles are per bundle ID)
    #[serde(default)]
    pub(super) provisioning_profile: String,
    /// Where a build of the project's own leaves the `.ipa`, when that isn't
    /// the project folder: a file or a folder, absolute or relative to the
    /// project. Empty = the project folder, where the built-in scripts put it.
    #[serde(default)]
    pub(super) ios_artifact_path: String,
    /// The same for the `.aab`, and for the `.apk` beside it.
    #[serde(default)]
    pub(super) android_artifact_path: String,
    // Store listing text and release settings
    /// Per-locale store text: locale code → texts
    #[serde(default)]
    pub(super) store_texts: std::collections::HashMap<String, StoreText>,
    /// Support URL sent with every App Store localization (required for review)
    #[serde(default)]
    pub(super) support_url: String,
    /// App uses only exempt encryption (HTTPS etc.) — answers export compliance
    #[serde(default)]
    pub(super) exempt_encryption: bool,
    /// Google Play track releases go to: internal / alpha / beta / production
    #[serde(default = "default_play_track")]
    pub(super) play_track: String,
    /// Google Play release status: draft / completed
    #[serde(default = "default_play_status")]
    pub(super) play_release_status: String,
    /// What was shipped, newest first
    #[serde(default)]
    pub(super) releases: Vec<ReleaseRecord>,
    /// Alias of this app's key in the upload keystore ("" = the project slug)
    #[serde(default)]
    pub(super) android_key_alias: String,
}

impl ProjectState {
    pub(super) fn with_defaults() -> Self {
        Self {
            primary_color: "#3B82F6".to_string(),
            secondary_color: "#FFFFFF".to_string(),
            locale: default_locale(),
            locales: default_locales(),
            export_ios: true,
            export_android: true,
            ios_targets: default_ios_targets(),
            android_targets: default_android_targets(),
            desktop_targets: default_desktop_targets(),
            auto_increment_build: true,
            play_track: default_play_track(),
            play_release_status: default_play_status(),
            ..Default::default()
        }
    }
    /// Ensure ios_targets / android_targets / desktop_targets vecs are the right length.
    /// Also migrate legacy `manual_texts` + `locale` into `locale_texts` / `locales`.
    pub(super) fn normalize_targets(&mut self) {
        while self.ios_targets.len() < IOS_TARGETS.len() {
            self.ios_targets.push(true);
        }
        self.ios_targets.truncate(IOS_TARGETS.len());
        while self.android_targets.len() < ANDROID_TARGETS.len() {
            self.android_targets.push(true);
        }
        self.android_targets.truncate(ANDROID_TARGETS.len());
        while self.desktop_targets.len() < DESKTOP_TARGETS.len() {
            self.desktop_targets.push(true);
        }
        self.desktop_targets.truncate(DESKTOP_TARGETS.len());
    }
    /// Migrate old shared data into the per-locale maps.
    pub(super) fn migrate_legacy(&mut self) {
        // Ensure `locales` is non-empty; default to en-US if blank
        if self.locales.is_empty() {
            self.locales = vec!["en-US".to_string()];
        }
        // Always ensure en-US is present as the default tab
        if !self.locales.contains(&"en-US".to_string()) {
            self.locales.insert(0, "en-US".to_string());
        }
        // Migrate legacy shared source_paths → en-US locale_sources.
        // IMPORTANT: drain() clears source_paths so re-opening the project never
        // re-adds paths the user has already deleted from locale_sources.
        if !self.source_paths.is_empty() {
            let entry = self.locale_sources.entry("en-US".to_string()).or_default();
            for p in self.source_paths.drain(..) {
                if !entry.contains(&p) {
                    entry.push(p);
                }
            }
            // source_paths is now empty — caller should persist this to disk.
        }
        // Migrate legacy shared manual_texts → en-US locale_texts
        if self
            .locale_texts
            .get("en-US")
            .map(|v| v.is_empty())
            .unwrap_or(true)
            && !self.manual_texts.is_empty()
        {
            self.locale_texts
                .insert("en-US".to_string(), self.manual_texts.clone());
        }
        // Seed locale_texts / locale_sources entries for every listed locale
        for loc in self.locales.clone() {
            let n = self.locale_sources.get(&loc).map(|v| v.len()).unwrap_or(0);
            self.ensure_texts_len(&loc, n);
            self.locale_sources.entry(loc).or_default();
        }
        // Keep legacy locale field in sync
        if let Some(first) = self.locales.first() {
            self.locale = first.clone();
        }
        // Migrate legacy single bundle_id → ios_bundle_id / android_bundle_id.
        // Only copy when the new fields are still empty (first open of an old project).
        if !self.bundle_id.is_empty() {
            if self.ios_bundle_id.is_empty() {
                self.ios_bundle_id = self.bundle_id.clone();
            }
            if self.android_bundle_id.is_empty() {
                self.android_bundle_id = self.bundle_id.clone();
            }
        }
    }
    /// Return the source paths for a given locale.
    pub(super) fn sources_for(&self, locale: &str) -> Vec<PathBuf> {
        self.locale_sources.get(locale).cloned().unwrap_or_default()
    }
    /// Return the texts for a given locale.
    pub(super) fn texts_for(&self, locale: &str) -> Vec<(String, String)> {
        self.locale_texts.get(locale).cloned().unwrap_or_default()
    }
    /// Ensure the text vec for `locale` has at least `n` entries.
    pub(super) fn ensure_texts_len(&mut self, locale: &str, n: usize) {
        let v = self.locale_texts.entry(locale.to_string()).or_default();
        while v.len() < n {
            v.push((String::new(), String::new()));
        }
    }
}

#[cfg(test)]
mod tests;
