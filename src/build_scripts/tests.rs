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
