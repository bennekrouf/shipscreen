use super::*;

#[test]
fn the_ios_script_carries_the_projects_own_values() {
    let s = script_ios_distribution(
        "Acme",
        "acme",
        "com.acme.app",
        "Apple Distribution: Acme (AB12CD34EF)",
        "/profiles/acme.mobileprovision",
        "1.4.0",
        27,
    );
    assert!(s.starts_with("#!/bin/bash\n"));
    assert!(s.contains("APP_NAME=\"Acme\""));
    assert!(s.contains("BUNDLE_ID=\"com.acme.app\""));
    assert!(s.contains("IDENTITY=\"Apple Distribution: Acme (AB12CD34EF)\""));
    assert!(s.contains("/profiles/acme.mobileprovision"));
    assert!(s.contains("1.4.0"));
    assert!(s.contains("27"));
}

#[test]
fn the_android_release_script_carries_the_package_and_version() {
    let s = script_android_release("Acme", "acme", "com.acme.android", "2.0.1", 41);
    assert!(s.starts_with("#!/bin/bash\n"));
    assert!(s.contains("com.acme.android"));
    assert!(s.contains("2.0.1"));
    assert!(s.contains("41"));
}

#[test]
fn the_android_scripts_build_in_the_projects_own_folder() {
    let s = script_android("Acme", "acme-app", "com.acme.android");
    assert!(s.contains("target/dx/acme-app/release/android/app"));
    let apk = script_build_apk("Acme", "acme", "com.acme.android");
    assert!(apk.contains("com.acme.android"));
    assert!(
        apk.contains("com.example.Acme"),
        "the package dx generates is replaced"
    );
}

#[test]
fn every_template_names_itself_as_shipscreens() {
    // `ensure_build_scripts` adds the marker; the templates must keep the shebang it goes after.
    for s in [
        script_ios_distribution("A", "a", "com.a", "id", "", "1.0", 1),
        script_android_release("A", "a", "com.a", "1.0", 1),
        script_android("A", "a", "com.a"),
        script_build_apk("A", "a", "com.a"),
    ] {
        assert!(s.starts_with("#!/bin/bash\n"));
    }
}

#[test]
fn every_script_uses_the_projects_own_dx_and_checks_its_version() {
    for s in [
        script_ios_distribution("A", "a", "com.a", "id", "", "1.0", 1),
        script_android_release("A", "a", "com.a", "1.0", 1),
        script_android("A", "a", "com.a"),
        script_build_apk("A", "a", "com.a"),
    ] {
        let check = s.find(DX_CHECK).expect("the dx check is in every script");
        let first_dx = s.find("dx b").expect("the script runs dx");
        assert!(check < first_dx, "the check comes before dx runs");
    }
}

/// The check itself, run by bash in a project whose Cargo.lock pins a
/// Dioxus the dx on PATH doesn't match, with and without a matching dx in
/// `.tools/bin`.
#[cfg(unix)]
#[test]
fn the_dx_check_prefers_the_projects_own_dx() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let root = std::env::temp_dir().join("shipscreen-test-dx-check");
    let _ = std::fs::remove_dir_all(&root);
    let fake_dx = |dir: &std::path::Path, version: &str| {
        std::fs::create_dir_all(dir).unwrap();
        let dx = dir.join("dx");
        std::fs::write(&dx, format!("#!/bin/sh\necho \"dioxus {version} (abc)\"\n")).unwrap();
        std::fs::set_permissions(&dx, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    let global = root.join("global");
    fake_dx(&global, "0.7.10");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("Cargo.lock"),
        "[[package]]\nname = \"dioxus\"\nversion = \"0.8.0-alpha.3\"\n",
    )
    .unwrap();
    let run = || {
        Command::new("bash")
            .arg("-c")
            .arg(DX_CHECK)
            .current_dir(&project)
            .env("PATH", format!("{}:/usr/bin:/bin", global.display()))
            .output()
            .unwrap()
    };

    let out = run();
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("dx is 0.7.10"), "{said}");
    assert!(said.contains("--root .tools"), "{said}");

    fake_dx(&project.join(".tools/bin"), "0.8.0-alpha.3");
    let out = run();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("dx: 0.8.0-alpha.3"), "{said}");
}
