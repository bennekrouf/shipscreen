use super::*;

#[test]
fn a_built_bundle_is_found_where_the_build_step_says_it_is() {
    let dir = std::env::temp_dir().join("shipscreen-test-artifacts");
    let _ = std::fs::remove_dir_all(&dir);
    let exported = dir.join("build/export/Tafseel.ipa");
    let bundled = dir.join("app/build/outputs/bundle/release/app-release.aab");
    std::fs::create_dir_all(exported.parent().unwrap()).unwrap();
    std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
    std::fs::write(&exported, "x").unwrap();
    std::fs::write(&bundled, "x").unwrap();

    // Nothing in the project folder, and nowhere else named to look.
    let found = scan_artifacts(&dir, "", "");
    assert!(found.ipa.is_none() && found.aab.is_none());

    // The file itself for the IPA, the folder it lands in for the AAB.
    let found = scan_artifacts(
        &dir,
        "build/export/Tafseel.ipa",
        "app/build/outputs/bundle/release",
    );
    assert_eq!(found.ipa.map(|a| a.path), Some(exported));
    assert_eq!(found.aab.map(|a| a.path), Some(bundled));

    // The project folder still counts, and the newest of a kind wins.
    std::fs::write(dir.join("Tafseel.ipa"), "x").unwrap();
    let found = scan_artifacts(&dir, "build/export/Tafseel.ipa", "");
    assert_eq!(found.ipa.map(|a| a.path), Some(dir.join("Tafseel.ipa")));

    assert_eq!(project_path(&dir, "   "), None);
    let absolute = std::env::temp_dir().join("shipscreen-test-elsewhere");
    assert_eq!(
        project_path(&dir, &absolute.to_string_lossy()),
        Some(absolute)
    );
}
