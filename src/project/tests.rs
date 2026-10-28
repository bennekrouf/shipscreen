use super::*;
use std::path::{Path, PathBuf};

/// A folder of its own under the system temp dir, removed first if a run left one.
fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("shipscreen-project-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_new_project_exports_every_target_for_both_stores() {
    let p = ProjectState::with_defaults();
    assert!(p.export_ios && p.export_android);
    assert_eq!(p.ios_targets.len(), crate::IOS_TARGETS.len());
    assert_eq!(p.android_targets.len(), crate::ANDROID_TARGETS.len());
    assert_eq!(p.desktop_targets.len(), crate::DESKTOP_TARGETS.len());
    assert!(p.auto_increment_build);
}

#[test]
fn target_lists_saved_by_another_version_are_padded_or_cut() {
    let mut p = ProjectState {
        ios_targets: vec![false],
        android_targets: vec![false; crate::ANDROID_TARGETS.len() + 3],
        ..Default::default()
    };
    p.normalize_targets();
    assert_eq!(p.ios_targets.len(), crate::IOS_TARGETS.len());
    assert!(!p.ios_targets[0], "a choice already made is kept");
    assert!(
        p.ios_targets[1..].iter().all(|&t| t),
        "a new target starts ticked"
    );
    assert_eq!(p.android_targets.len(), crate::ANDROID_TARGETS.len());
    assert_eq!(p.desktop_targets.len(), crate::DESKTOP_TARGETS.len());
}

#[test]
fn english_is_always_a_language_and_comes_first_when_added() {
    let mut empty = ProjectState::default();
    empty.migrate_legacy();
    assert_eq!(empty.locales, ["en-US"]);
    assert_eq!(empty.locale, "en-US");

    let mut french = ProjectState {
        locales: vec!["fr-FR".into()],
        ..Default::default()
    };
    french.migrate_legacy();
    assert_eq!(french.locales, ["en-US", "fr-FR"]);
    assert_eq!(
        french.locale, "en-US",
        "the old single locale follows the first tab"
    );
}

#[test]
fn old_shared_screenshots_move_to_english_once() {
    let a = PathBuf::from("/shots/a.png");
    let b = PathBuf::from("/shots/b.png");
    let mut p = ProjectState {
        source_paths: vec![a.clone(), b.clone(), a.clone()],
        ..Default::default()
    };
    p.locale_sources.insert("en-US".into(), vec![a.clone()]);
    p.migrate_legacy();
    assert!(
        p.source_paths.is_empty(),
        "drained, so a deleted screenshot never comes back"
    );
    assert_eq!(p.sources_for("en-US"), [a, b], "nothing listed twice");
}

#[test]
fn old_shared_texts_become_english_unless_english_has_its_own() {
    let old = vec![("Welcome".to_string(), "Start here".to_string())];
    let mut p = ProjectState {
        manual_texts: old.clone(),
        ..Default::default()
    };
    p.migrate_legacy();
    assert_eq!(p.texts_for("en-US"), old);

    let mine = vec![("Hello".to_string(), String::new())];
    let mut kept = ProjectState {
        manual_texts: old,
        ..Default::default()
    };
    kept.locale_texts.insert("en-US".into(), mine.clone());
    kept.migrate_legacy();
    assert_eq!(kept.texts_for("en-US"), mine);
}

#[test]
fn every_language_gets_a_text_for_each_of_its_screenshots() {
    let mut p = ProjectState {
        locales: vec!["en-US".into(), "de-DE".into()],
        ..Default::default()
    };
    p.locale_sources.insert(
        "de-DE".into(),
        vec![PathBuf::from("/1.png"), PathBuf::from("/2.png")],
    );
    p.migrate_legacy();
    assert_eq!(p.texts_for("de-DE").len(), 2);
    assert!(
        p.sources_for("en-US").is_empty(),
        "an entry is seeded for every language"
    );
    assert!(
        p.texts_for("it-IT").is_empty(),
        "an unknown language has nothing"
    );
}

#[test]
fn the_old_single_bundle_id_fills_both_stores_but_never_overwrites() {
    let mut fresh = ProjectState {
        bundle_id: "com.acme.app".into(),
        ..Default::default()
    };
    fresh.migrate_legacy();
    assert_eq!(fresh.ios_bundle_id, "com.acme.app");
    assert_eq!(fresh.android_bundle_id, "com.acme.app");

    let mut set = ProjectState {
        bundle_id: "com.acme.app".into(),
        android_bundle_id: "com.acme.android".into(),
        ..Default::default()
    };
    set.migrate_legacy();
    assert_eq!(set.android_bundle_id, "com.acme.android");
}

#[test]
fn texts_grow_to_the_length_asked_and_never_shrink() {
    let mut p = ProjectState::default();
    p.ensure_texts_len("en-US", 3);
    assert_eq!(p.texts_for("en-US").len(), 3);
    p.ensure_texts_len("en-US", 1);
    assert_eq!(p.texts_for("en-US").len(), 3);
}

#[test]
fn the_state_file_keeps_its_old_name_in_a_project_that_has_one() {
    let dir = scratch("names");
    assert_eq!(crate::project_state_path(&dir), dir.join("shipscreen.json"));
    std::fs::write(dir.join("appscreens.json"), "{}").unwrap();
    assert_eq!(crate::project_state_path(&dir), dir.join("appscreens.json"));
    std::fs::write(dir.join("shipscreen.json"), "{}").unwrap();
    assert_eq!(
        crate::project_state_path(&dir),
        dir.join("shipscreen.json"),
        "the new name wins"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_project_reads_back_what_was_saved() {
    let dir = scratch("round-trip");
    let mut p = crate::load_project_state(&dir);
    assert_eq!(
        p.locales,
        ["en-US"],
        "a project with no file starts from the defaults"
    );
    p.app_name = "Acme".into();
    p.locales.push("fr-FR".into());
    crate::save_project_state(&dir, &p);
    let back = crate::load_project_state(&dir);
    assert_eq!(back.app_name, "Acme");
    assert_eq!(back.locales, ["en-US", "fr-FR"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn migrated_screenshots_are_saved_so_a_deletion_sticks() {
    let dir = scratch("migrated");
    let old = serde_json::json!({ "source_paths": ["/shots/a.png"] });
    std::fs::write(dir.join("shipscreen.json"), old.to_string()).unwrap();
    let p = crate::load_project_state(&dir);
    assert_eq!(p.sources_for("en-US"), [PathBuf::from("/shots/a.png")]);
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("shipscreen.json")).unwrap())
            .unwrap();
    assert_eq!(on_disk["source_paths"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_write_by_another_process_is_noticed_once() {
    let dir = scratch("elsewhere");
    let p = crate::load_project_state(&dir);
    crate::save_project_state(&dir, &p);
    assert!(
        !crate::project_state_changed_elsewhere(&dir),
        "our own save is not someone else's"
    );
    // Another process: a write this one did not note, with a later time.
    let file = crate::project_state_path(&dir);
    std::fs::write(&file, serde_json::to_string(&p).unwrap()).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(crate::project_state_changed_elsewhere(&dir));
    assert!(
        !crate::project_state_changed_elsewhere(&dir),
        "reported once"
    );
    let _ = std::fs::remove_dir_all(Path::new(&dir));
}
