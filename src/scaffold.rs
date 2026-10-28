//! Keeping a project's build scripts current, and creating a new project.

use super::*;

/// (Re)write ShipScreen' build scripts from the latest template, so config
/// changes and template fixes are always picked up. Scripts the developer
/// wrote are left alone, and a project the templates cannot build gets none.
#[allow(clippy::too_many_arguments)]
pub(super) fn ensure_build_scripts(
    project_dir: &Path,
    app_name: &str,
    project_slug: &str,
    ios_bundle_id: &str,
    android_bundle_id: &str,
    identity: &str,
    provisioning_profile: &str,
    version: &str,
    ios_build_number: u32,
    android_version_code: u32,
) -> Vec<String> {
    // Only a Dioxus project gets the templates: in any other project they
    // would be four scripts that cannot build it, and the one whose name
    // ShipScreen takes is the one the developer needs for their own.
    if !is_dioxus_project(project_dir) {
        return Vec::new();
    }
    let scripts: &[(&str, String)] = &[
        (
            "build_ios_distribution.sh",
            script_ios_distribution(
                app_name,
                project_slug,
                ios_bundle_id,
                identity,
                provisioning_profile,
                version,
                ios_build_number,
            ),
        ),
        (
            "build_android_release.sh",
            script_android_release(
                app_name,
                project_slug,
                android_bundle_id,
                version,
                android_version_code,
            ),
        ),
        (
            "build_android.sh",
            script_android(app_name, project_slug, android_bundle_id),
        ),
        (
            "build_apk.sh",
            script_build_apk(app_name, project_slug, android_bundle_id),
        ),
    ];

    // A script ShipScreen didn't write is the developer's: it may carry fixes
    // the template doesn't have (a display name, a minimum iOS), so it's used
    // as-is, never replaced. Older ShipScreen scripts are recognised by name.
    let read = |name: &str| std::fs::read_to_string(project_dir.join(name)).ok();
    let generated_here = scripts
        .iter()
        .any(|(n, _)| read(n).is_some_and(|t| written_by_us(&t)));
    let ours = |name: &str| match read(name) {
        None => true,
        Some(text) => {
            written_by_us(&text)
                || (generated_here && matches!(name, "build_android.sh" | "build_apk.sh"))
        }
    };

    let mut created = Vec::new();
    for (name, content) in scripts {
        let path = project_dir.join(name);
        if !ours(name) {
            continue;
        }
        let content = content.replacen(
            "#!/bin/bash\n",
            &format!("#!/bin/bash\n{SCRIPT_MARKER}\n"),
            1,
        );
        if std::fs::write(&path, content).is_ok() {
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
            created.push((*name).to_string());
        }
    }
    created
}

/// Create the minimal Rust/Dioxus project structure for a new app.
/// Returns Ok(()) or an Err with a description.
pub(super) fn scaffold_new_project(
    dir: &Path,
    name: &str,
    slug: &str,
    bundle_id: &str,
    platform: &PlatformType,
) -> Result<(), String> {
    let src_dir = dir.join("src");
    let assets_dir = dir.join("assets");

    std::fs::create_dir_all(&src_dir).map_err(|e| format!("Cannot create src/: {e}"))?;
    std::fs::create_dir_all(&assets_dir).map_err(|e| format!("Cannot create assets/: {e}"))?;

    // ── Cargo.toml ────────────────────────────────────────────────────────────
    let features = match platform {
        PlatformType::Desktop => r#"["desktop"]"#,
        PlatformType::Ios => r#"["mobile"]"#,
        PlatformType::Android => r#"["mobile"]"#,
        PlatformType::IosAndroid => r#"["mobile", "router"]"#,
    };
    let cargo_toml = format!(
        "[package]\nname = \"{slug}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\ndioxus = {{ version = \"0.6\", features = {features} }}\n"
    );
    std::fs::write(dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("Cannot write Cargo.toml: {e}"))?;

    // ── Dioxus.toml ───────────────────────────────────────────────────────────
    let default_platform = match platform {
        PlatformType::Desktop => "desktop",
        _ => "mobile",
    };
    let dioxus_toml = format!(
        "[application]\nname = \"{slug}\"\ndefault_platform = \"{default_platform}\"\n\n\
         [web.app]\ntitle = \"{name}\"\n\n\
         [web.resource]\nstyle = []\nscript = []\n\n\
         [bundle]\nidentifier = \"{bundle_id}\"\npublisher = \"Mayorana\"\n\
         icon = [\"assets/icon.png\"]\nname = \"{name}\"\n"
    );
    std::fs::write(dir.join("Dioxus.toml"), &dioxus_toml)
        .map_err(|e| format!("Cannot write Dioxus.toml: {e}"))?;

    // ── src/main.rs ───────────────────────────────────────────────────────────
    let main_rs = format!(
        "use dioxus::prelude::*;\n\nfn main() {{\n    dioxus::launch(App);\n}}\n\n\
         #[component]\nfn App() -> Element {{\n    rsx! {{\n        div {{\n\
             h1 {{ \"{name}\" }}\n            p {{ \"Hello, world!\" }}\n        }}\n    }}\n}}\n"
    );
    std::fs::write(src_dir.join("main.rs"), &main_rs)
        .map_err(|e| format!("Cannot write src/main.rs: {e}"))?;

    // ── .gitignore ────────────────────────────────────────────────────────────
    let gitignore = "/target\nshipscreen.json\n.env\n";
    // Only create if not already present
    let gi_path = dir.join(".gitignore");
    if !gi_path.exists() {
        let _ = std::fs::write(gi_path, gitignore);
    }

    // ── Entitlements.plist (iOS only) ─────────────────────────────────────────
    if platform.has_ios() {
        let entitlements = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"\n\
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n<dict>\n\
             \t<key>application-identifier</key>\n\
             \t<string>{bundle_id}</string>\n\
             \t<key>get-task-allow</key>\n\
             \t<false/>\n\
             </dict>\n</plist>\n"
        );
        let _ = std::fs::write(dir.join("Entitlements.plist"), &entitlements);
        let _ = std::fs::write(dir.join("build_number.txt"), "1\n");
    }

    Ok(())
}
