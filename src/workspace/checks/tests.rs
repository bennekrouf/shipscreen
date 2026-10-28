use super::*;

const PROFILE: &str = r#"garbage-cms-header<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CreationDate</key>
    <date>2025-01-01T10:00:00Z</date>
    <key>Entitlements</key>
    <dict>
        <key>application-identifier</key>
        <string>AB12CD34EF.com.acme.app</string>
        <key>get-task-allow</key>
        <false/>
    </dict>
    <key>ExpirationDate</key>
    <date>2026-01-01T10:00:00Z</date>
    <key>Name</key>
    <string>Acme App Store</string>
    <key>TeamIdentifier</key>
    <array>
        <string>AB12CD34EF</string>
    </array>
</dict></plist>trailing-signature"#;

fn write_profile(name: &str, body: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("shipscreen-test-{name}.mobileprovision"));
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn reads_app_store_profile() {
    let info = read_profile(&write_profile("appstore", PROFILE)).unwrap();
    assert_eq!(info.name, "Acme App Store");
    assert_eq!(info.team_id, "AB12CD34EF");
    assert_eq!(info.app_id, "com.acme.app");
    assert_eq!(info.expires, "2026-01-01");
    assert_eq!(info.kind, ProfileKind::AppStore);
    assert!(info.matches_bundle("com.acme.app"));
    assert!(!info.matches_bundle("com.acme.other"));
}

#[test]
fn classifies_profile_kinds() {
    let dev = PROFILE.replace(
        "<key>get-task-allow</key>\n        <false/>",
        "<key>get-task-allow</key>\n        <true/>",
    );
    assert_eq!(
        read_profile(&write_profile("dev", &dev)).unwrap().kind,
        ProfileKind::Development
    );
    let adhoc = PROFILE.replace(
        "<key>Name</key>",
        "<key>ProvisionedDevices</key><array><string>x</string></array><key>Name</key>",
    );
    assert_eq!(
        read_profile(&write_profile("adhoc", &adhoc)).unwrap().kind,
        ProfileKind::AdHoc
    );
}

#[test]
fn installs_a_profile_under_its_uuid() {
    let body = PROFILE.replace(
        "<key>Name</key>",
        "<key>UUID</key>\n    <string>0bc1-test-uuid</string>\n    <key>Name</key>",
    );
    let folder = std::env::temp_dir().join("shipscreen-test-profiles");
    let _ = std::fs::remove_dir_all(&folder);
    let path = install_profile_in(&folder, body.as_bytes()).unwrap();
    assert_eq!(path, folder.join("0bc1-test-uuid.mobileprovision"));
    assert_eq!(read_profile(&path).unwrap().app_id, "com.acme.app");
    assert!(install_profile_in(&folder, b"hello").is_err());
}

#[test]
fn wildcard_profiles_match_prefix() {
    let wild = PROFILE.replace("AB12CD34EF.com.acme.app", "AB12CD34EF.com.acme.*");
    let info = read_profile(&write_profile("wild", &wild)).unwrap();
    assert!(info.matches_bundle("com.acme.anything"));
    assert!(!info.matches_bundle("org.other.app"));
}

#[test]
fn checks_team_against_identity() {
    let info = read_profile(&write_profile("team", PROFILE)).unwrap();
    let checks = profile_checks(
        &info,
        "com.acme.app",
        "Apple Distribution: Jane (ZZ99ZZ99ZZ)",
    );
    let team = checks
        .iter()
        .find(|c| c.label == "Same team as signing identity")
        .unwrap();
    assert!(!team.ok);
}

#[test]
fn today_is_a_plausible_date() {
    let t = utc_today();
    assert_eq!(t.len(), 10);
    assert!(t.as_str() > "2024-01-01" && t.as_str() < "2100-01-01");
}

#[test]
fn version_rules() {
    assert!(valid_version("1"));
    assert!(valid_version("1.2"));
    assert!(valid_version("1.2.3"));
    assert!(!valid_version(""));
    assert!(!valid_version("1.2.3.4"));
    assert!(!valid_version("1.2-beta"));
    assert!(!valid_version("1..2"));
    assert_eq!(bump_version("1.2.3", 0), "2.0.0");
    assert_eq!(bump_version("1.2.3", 1), "1.3.0");
    assert_eq!(bump_version("1.2", 2), "1.2.1");
}

#[test]
fn android_output_is_per_language_inside_the_project() {
    let project = std::env::temp_dir().join("shipscreen-test-android-out");
    let _ = std::fs::remove_dir_all(&project);
    let ios_dir = project.join("fastlane/screenshots/ios");
    let android_root = project.join("fastlane/metadata/android");
    let png = {
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbaImage::from_pixel(40, 80, Rgba([10, 20, 30, 255]))
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let targets = ExportTargets {
        ios_dir,
        android_root: android_root.clone(),
        desktop_dir: project.join("fastlane/screenshots/desktop"),
        ios: vec![],
        android: vec![true; 2],
        desktop: vec![],
    };
    let mut all = Vec::new();
    for (locale, play) in [("en-US", "en-US"), ("zh-Hans", "zh-CN")] {
        for screen in 1..=2 {
            all.extend(resize_to_targets(&png, screen, locale, play, &targets).unwrap());
        }
    }
    // 2 languages × (2 phone + 1 feature graphic), all distinct files in the project.
    assert_eq!(all.len(), 6);
    let paths: std::collections::HashSet<_> = all.iter().map(|(_, p)| p.clone()).collect();
    assert_eq!(paths.len(), 6);
    assert!(paths
        .iter()
        .all(|p| p.starts_with(&android_root) && p.exists()));
    assert!(android_root
        .join("zh-CN/images/phoneScreenshots/02.png")
        .exists());
    assert!(android_root
        .join("en-US/images/featureGraphic.png")
        .exists());
    assert!(all
        .iter()
        .all(|(l, _)| l.contains("[en-US]") || l.contains("[zh-Hans]")));
}

#[test]
fn desktop_sizes_are_exported_and_unticked_platforms_are_not() {
    let project = std::env::temp_dir().join("shipscreen-test-desktop-out");
    let _ = std::fs::remove_dir_all(&project);
    let mut p = ProjectState::with_defaults();
    p.platform_type = PlatformType::Desktop;
    p.export_ios = false;
    p.export_android = false;
    p.desktop_targets = vec![true, false, true];
    let targets = ExportTargets::for_project(&project, &p);
    let png = {
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbaImage::from_pixel(160, 100, Rgba([10, 20, 30, 255]))
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let out = resize_to_targets(&png, 1, "en-US", "en-US", &targets).unwrap();
    // Only the two ticked desktop sizes — no iOS or Android files.
    assert_eq!(out.len(), 2);
    assert!(out
        .iter()
        .all(|(l, _)| l.starts_with("Desktop ") && l.contains("[en-US]")));
    let mac = project.join("fastlane/screenshots/desktop/en-US/desktop_mac-01.png");
    assert_eq!(
        (
            image::open(&mac).unwrap().width(),
            image::open(&mac).unwrap().height()
        ),
        (1280, 800)
    );
    let wide = project.join("fastlane/screenshots/desktop/en-US/desktop_wide-01.png");
    assert_eq!(
        (
            image::open(&wide).unwrap().width(),
            image::open(&wide).unwrap().height()
        ),
        (1920, 1080)
    );
}

#[test]
fn android_release_script_takes_signing_from_the_environment() {
    let script = script_android_release("Nahw", "nahw", "com.example.nahw", "1.0.0", 6);
    assert!(
        !script.contains("Salma"),
        "no password in the generated script"
    );
    assert!(!script.contains("mayorana-release.keystore"));
    assert!(script.contains(r#"KEY_ALIAS="${ANDROID_KEY_ALIAS:-nahw}""#));
    assert!(script.contains(r#"KEYSTORE_PATH="${ANDROID_KEYSTORE_PATH:-}""#));
    // The template is format!-escaped bash; make sure it still parses.
    let path = std::env::temp_dir().join("shipscreen-test-android-release.sh");
    std::fs::write(&path, &script).unwrap();
    let check = std::process::Command::new("bash")
        .arg("-n")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn detects_an_existing_project_from_its_files() {
    let dir = std::env::temp_dir().join("shipscreen-test-detect");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target/dx/tafseel-quran/release/ios")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"tafseel-quran\"\nversion = \"1.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Dioxus.toml"),
        "[application]\nname = \"TafseelQuran\"\n\n[mobile]\ntitle = \"Tafseel\"\nbundle_identifier = \"com.mayorana.tafseel\"\n\n\
         [mobile.ios]\n# live is 1.0\nversion = \"1.1\"\nbuild_number = 12\n\n[bundle]\nidentifier = \"com.mayorana.tafseel\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("TafseelQuran.ipa"), "x").unwrap();
    std::fs::write(dir.join("build_number.txt"), "12\n").unwrap();

    // What ShipScreen had saved before it could detect anything.
    let mut p = ProjectState::with_defaults();
    p.version = "1.0".into();
    assert!(seed_identity(&mut p, &dir));
    assert_eq!(p.app_name, "Tafseel");
    assert_eq!(
        p.project_slug, "tafseel-quran",
        "dx's folder, not Dioxus.toml's name"
    );
    assert_eq!(p.ios_bundle_id, "com.mayorana.tafseel");
    assert_eq!(p.android_bundle_id, "com.mayorana.tafseel");
    assert_eq!(
        p.platform_type,
        PlatformType::Ios,
        "only iOS was ever built"
    );
    assert_eq!(p.version, "1.1");

    // Once set, nothing is overwritten.
    p.app_name = "Tafseel Quran".into();
    p.version = "1.2".into();
    assert!(!seed_identity(&mut p, &dir));
    assert_eq!(
        (p.app_name.as_str(), p.version.as_str()),
        ("Tafseel Quran", "1.2")
    );
}

#[test]
fn a_build_script_and_the_last_android_build_win() {
    let dir = std::env::temp_dir().join("shipscreen-test-detect-scripts");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target/dx/nahw/release/android/app/app")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"nahw\"\n").unwrap();
    std::fs::write(
        dir.join("Dioxus.toml"),
        "[bundle]\nidentifier = \"com.example.nahw\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("build_ios_distribution.sh"),
        "#!/bin/bash\nAPP_NAME=\"Nahw\"\nVERSION=\"2.0.1\"\nBUNDLE_ID=\"com.mayorana.tafseel.nahw\"\nDX=\"${DX:-dx}\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("target/dx/nahw/release/android/app/app/build.gradle.kts"),
        "android {\n    defaultConfig {\n        applicationId = \"com.mayorana.nahw\"\n    }\n}\n",
    )
    .unwrap();
    let d = detect_identity(&dir);
    assert_eq!(d.app_name.as_deref(), Some("Nahw"));
    assert_eq!(
        d.ios_bundle_id.as_deref(),
        Some("com.mayorana.tafseel.nahw")
    );
    assert_eq!(d.android_bundle_id.as_deref(), Some("com.mayorana.nahw"));
    assert_eq!(d.version.as_deref(), Some("2.0.1"));
    assert!(d.has_ios && d.has_android);
    assert_eq!(
        shell_var("DX=\"${DX:-dx}\"", "DX"),
        None,
        "not a plain value"
    );
}

#[test]
fn never_replaces_a_build_script_it_did_not_write() {
    let dir = std::env::temp_dir().join("shipscreen-test-own-scripts");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Dioxus.toml"), "[bundle]\n").unwrap();
    let mine = "#!/bin/bash\n# my tuned script\nDISPLAY_NAME=\"Tafseel\"\n";
    std::fs::write(dir.join("build_ios_distribution.sh"), mine).unwrap();
    let written = ensure_build_scripts(
        &dir,
        "Tafseel",
        "tafseel-quran",
        "com.a",
        "com.a",
        "id",
        "",
        "1.1",
        13,
        1,
    );
    assert!(!written.contains(&"build_ios_distribution.sh".to_string()));
    assert_eq!(
        std::fs::read_to_string(dir.join("build_ios_distribution.sh")).unwrap(),
        mine
    );
    // The ones it wrote carry the marker, so they're replaced next time.
    let apk = std::fs::read_to_string(dir.join("build_apk.sh")).unwrap();
    assert!(apk.starts_with(&format!("#!/bin/bash\n{SCRIPT_MARKER}\n")));
    let again = ensure_build_scripts(
        &dir,
        "Tafseel",
        "tafseel-quran",
        "com.a",
        "com.a",
        "id",
        "",
        "1.1",
        14,
        2,
    );
    assert_eq!(again.len(), 3);
}

#[test]
fn seeds_numbers_after_the_last_used_ones() {
    let dir = std::env::temp_dir().join("shipscreen-test-seed");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("build_number.txt"), "14\n").unwrap();
    std::fs::write(
        dir.join("Dioxus.toml"),
        "[android]\nversion_code = 36\nversion_name = \"2.1.0\"\n",
    )
    .unwrap();
    let mut p = ProjectState::with_defaults();
    assert!(seed_release_fields(&mut p, &dir, &Settings::default()));
    assert_eq!(p.version, "2.1.0");
    assert_eq!(p.ios_build_number, 15);
    assert_eq!(p.android_version_code, 37);
    // Already seeded: nothing changes.
    assert!(!seed_release_fields(&mut p, &dir, &Settings::default()));
}

#[test]
fn reads_an_xcode_project_for_what_the_app_is() {
    let dir = std::env::temp_dir().join("shipscreen-test-xcode");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Tafseel.xcodeproj")).unwrap();
    std::fs::write(
        dir.join("Tafseel.xcodeproj/project.pbxproj"),
        "\t\t\t\tCURRENT_PROJECT_VERSION = 12;\n\
         \t\t\t\tINFOPLIST_KEY_CFBundleDisplayName = Tafseel;\n\
         \t\t\t\tMARKETING_VERSION = 2.4.1;\n\
         \t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = com.mayorana.tafseel;\n\
         \t\t\t\tPRODUCT_NAME = \"$(TARGET_NAME)\";\n",
    )
    .unwrap();
    let d = detect_identity(&dir);
    assert_eq!(d.app_name.as_deref(), Some("Tafseel"));
    assert_eq!(d.ios_bundle_id.as_deref(), Some("com.mayorana.tafseel"));
    assert_eq!(d.version.as_deref(), Some("2.4.1"));
    assert!(d.has_ios && !d.has_android);
    // A setting Xcode works out from another one is no answer.
    assert_eq!(
        pbx_value("\t\t\t\tPRODUCT_NAME = \"$(TARGET_NAME)\";", "PRODUCT_NAME"),
        None
    );

    // The build number carries on from the one Xcode last used.
    let mut p = ProjectState::default();
    assert!(seed_release_fields(&mut p, &dir, &Settings::default()));
    assert_eq!(p.ios_build_number, 13);
}

#[test]
fn reads_a_gradle_project_in_either_language() {
    let dir = std::env::temp_dir().join("shipscreen-test-gradle");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("app/src/main/res/values")).unwrap();
    std::fs::write(
        dir.join("app/src/main/res/values/strings.xml"),
        "<resources>\n    <string name=\"app_name\">Mufrad</string>\n</resources>\n",
    )
    .unwrap();
    // Groovy: no `=`, and a key that only starts the same doesn't count.
    std::fs::write(
        dir.join("app/build.gradle"),
        "android {\n    defaultConfig {\n        applicationId \"com.mayorana.mufrad\"\n\
                 applicationIdSuffix \".debug\"\n        versionCode 7\n\
                 versionName \"1.4.0\"\n    }\n}\n",
    )
    .unwrap();
    let d = detect_identity(&dir);
    assert_eq!(d.app_name.as_deref(), Some("Mufrad"));
    assert_eq!(d.android_bundle_id.as_deref(), Some("com.mayorana.mufrad"));
    assert_eq!(d.version.as_deref(), Some("1.4.0"));
    assert!(d.has_android && !d.has_ios);
    let mut p = ProjectState::default();
    assert!(seed_release_fields(&mut p, &dir, &Settings::default()));
    assert_eq!(p.android_version_code, 8);

    // Kotlin: the same answers out of `key = value`.
    std::fs::remove_file(dir.join("app/build.gradle")).unwrap();
    std::fs::write(
        dir.join("app/build.gradle.kts"),
        "android {\n    namespace = \"com.mayorana.mufrad\"\n\
             defaultConfig {\n        applicationId = \"com.mayorana.mufrad\"\n\
                 versionName = \"1.4.0\"\n    }\n}\n",
    )
    .unwrap();
    let d = detect_identity(&dir);
    assert_eq!(d.android_bundle_id.as_deref(), Some("com.mayorana.mufrad"));
    assert_eq!(d.version.as_deref(), Some("1.4.0"));
}

#[test]
fn writes_no_scripts_into_a_project_the_templates_cannot_build() {
    let dir = std::env::temp_dir().join("shipscreen-test-swift-project");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // An Xcode project: a folder with no Cargo package in it.
    std::fs::write(dir.join("Tafseel.xcodeproj"), "").unwrap();
    assert!(!is_dioxus_project(&dir));
    let written = ensure_build_scripts(
        &dir, "Tafseel", "tafseel", "com.a", "com.a", "id", "", "1.1", 13, 1,
    );
    assert!(written.is_empty());
    assert!(!dir.join("build_ios_distribution.sh").exists());
    assert!(!dir.join("build_android_release.sh").exists());

    // A Cargo project that uses Dioxus is one they can build.
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"tafseel\"\n\n[dependencies]\ndioxus = \"0.7\"\n",
    )
    .unwrap();
    assert!(is_dioxus_project(&dir));
    assert_eq!(
        ensure_build_scripts(&dir, "Tafseel", "tafseel", "com.a", "com.a", "id", "", "1.1", 13, 1,)
            .len(),
        4
    );
}

#[test]
fn only_a_script_shipscreen_wrote_carries_its_numbers() {
    let dir = std::env::temp_dir().join("shipscreen-test-whose-script");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Nothing there: ShipScreen's internal Android build ran.
    assert!(script_is_ours(&dir, "build_android_release.sh"));
    std::fs::write(
        dir.join("build_android_release.sh"),
        "#!/bin/bash\n./gradlew bundleRelease\n",
    )
    .unwrap();
    assert!(!script_is_ours(&dir, "build_android_release.sh"));
    std::fs::write(
        dir.join("build_android_release.sh"),
        format!("#!/bin/bash\n{SCRIPT_MARKER}\n"),
    )
    .unwrap();
    assert!(script_is_ours(&dir, "build_android_release.sh"));

    // A script for another shell is the developer's: ShipScreen writes
    // only .sh, so when one of those runs, the numbers are its own.
    std::fs::write(
        dir.join("build_android_release.ps1"),
        "gradlew bundleRelease\n",
    )
    .unwrap();
    std::fs::remove_file(dir.join("build_android_release.sh")).unwrap();
    assert_eq!(
        build_script(&dir, "build_android_release.sh"),
        Some(dir.join("build_android_release.ps1"))
    );
    assert!(!script_is_ours(&dir, "build_android_release.sh"));
    assert_eq!(build_script(&dir, "build_apk.sh"), None);
}

#[test]
fn a_project_from_before_the_setting_still_gets_its_icon_copied() {
    let p: ProjectState = serde_json::from_str("{}").unwrap();
    assert_eq!(p.logo_dest, "assets/icon.png");
}
