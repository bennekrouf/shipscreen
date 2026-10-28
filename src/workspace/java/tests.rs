use super::*;

fn jdk(major: u32, source: &'static str) -> Jdk {
    Jdk {
        home: format!("/jdk/{source}/{major}").into(),
        major,
        version: format!("{major}.0.1"),
        source,
    }
}

#[test]
fn builds_never_use_a_java_gradle_cannot_run() {
    let (j26, j25, j21) = (
        jdk(26, "Homebrew"),
        jdk(25, "Android Studio"),
        jdk(21, "Homebrew"),
    );
    let all = vec![j26.clone(), j25.clone(), j21.clone()];
    let g91 = (9, 1);

    // Nothing chosen: the recommended LTS.
    assert_eq!(pick_for_build(None, None, &all, g91).0.unwrap().major, 21);
    // A compatible choice is respected, silently.
    let (j, notes) = pick_for_build(Some(j25.clone()), None, &all, g91);
    assert_eq!((j.unwrap().major, notes.len()), (25, 0));
    // Java 26 picked (or in JAVA_HOME) falls back to 21, and says so.
    let (j, notes) = pick_for_build(Some(j26.clone()), None, &all, g91);
    assert_eq!(j.unwrap().major, 21);
    assert!(
        notes[0].contains("too new for Gradle 9.1") && notes[0].contains("Java 21"),
        "{notes:?}"
    );
    let (j, notes) = pick_for_build(None, Some(j26.clone()), &all, g91);
    assert_eq!((j.unwrap().major, notes.len()), (21, 1));
    // A compatible JAVA_HOME is used when nothing is chosen.
    assert_eq!(
        pick_for_build(None, Some(j25), &all, g91).0.unwrap().major,
        25
    );
    // Only Java 26 installed: nothing fits, still returned so the build can report it.
    let (j, notes) = pick_for_build(None, None, &[j26], g91);
    assert_eq!(j.unwrap().major, 26);
    assert!(notes.is_empty());
}

#[test]
fn diagnoses_java_failures_in_build_logs() {
    let log = |s: &str| vec!["> Task :app:compile".to_string(), s.to_string()];
    let d = diagnose_build_log(&log("BUG! exception in phase 'semantic analysis' in source unit '_BuildScript_' Unsupported class file major version 70"));
    assert_eq!(d, Some(BuildJavaIssue::Incompatible(26)));
    assert!(d
        .unwrap()
        .explain((9, 1))
        .contains("too new for Gradle 9.1"));
    assert_eq!(
        diagnose_build_log(&log(
            "Your build is currently configured to use incompatible Java 26.0.1 and Gradle 9.1.0."
        )),
        Some(BuildJavaIssue::Incompatible(26))
    );
    assert_eq!(
        diagnose_build_log(&log(
            "  Android Gradle plugin requires Java 17 to run. You are currently using Java 11."
        )),
        Some(BuildJavaIssue::NeedsAtLeast(17))
    );
    assert_eq!(
        diagnose_build_log(&log("   > Dependency requires at least JVM runtime version 17. This build uses a Java 11 JVM.")),
        Some(BuildJavaIssue::NeedsAtLeast(17))
    );
    assert_eq!(
        diagnose_build_log(&log(
            "The operation couldn’t be completed. Unable to locate a Java Runtime."
        )),
        Some(BuildJavaIssue::Missing)
    );
    assert_eq!(
        diagnose_build_log(&log("error: linking with `cc` failed")),
        None
    );

    let all = vec![
        jdk(26, "Homebrew"),
        jdk(25, "Android Studio"),
        jdk(21, "Homebrew"),
        jdk(11, "System"),
    ];
    assert_eq!(
        remedy(&all, (9, 1), &BuildJavaIssue::Incompatible(26))
            .unwrap()
            .major,
        21
    );
    assert_eq!(
        remedy(&all, (9, 1), &BuildJavaIssue::Incompatible(21))
            .unwrap()
            .major,
        25
    );
    assert_eq!(
        remedy(
            &[jdk(26, "Homebrew")],
            (9, 1),
            &BuildJavaIssue::Incompatible(26)
        ),
        None
    );
}

#[test]
fn install_choices_fit_the_machine() {
    assert_eq!(install_major((9, 1)), 21);
    assert_eq!(install_major((8, 3)), 17);

    let brew = Installers {
        brew: Some("/opt/homebrew/bin/brew".into()),
        android_studio: true,
        ..Default::default()
    };
    let mac = install_options("macos", 21, &brew);
    assert!(
        matches!(&mac[0].runner, Runner::InApp { args, .. } if args == &["install", "openjdk@21"])
    );
    assert!(matches!(&mac[1].runner, Runner::Browser(u) if u.contains("version=21&os=mac")));
    assert_eq!(
        mac.len(),
        2,
        "no Android Studio suggestion when it's installed"
    );

    let bare_mac = install_options("macos", 21, &Installers::default());
    assert!(matches!(bare_mac[0].runner, Runner::Browser(_)));
    assert!(bare_mac.iter().any(|o| o.title == "Android Studio"));

    let apt = Installers {
        linux_pm: Some("apt"),
        android_studio: true,
        ..Default::default()
    };
    let linux = install_options("linux", 17, &apt);
    assert_eq!(linux[0].command, "sudo apt-get install -y openjdk-17-jdk");
    assert_eq!(linux[0].runner, Runner::Terminal);

    let win = install_options(
        "windows",
        21,
        &Installers {
            winget: true,
            android_studio: true,
            ..Default::default()
        },
    );
    assert!(win[0].command.contains("EclipseAdoptium.Temurin.21.JDK"));
}

#[test]
fn parses_java_versions() {
    assert_eq!(
        parse_java_version("openjdk version \"21.0.12.1\" 2026-08-18\nOpenJDK Runtime"),
        Some(("21.0.12.1".into(), 21))
    );
    assert_eq!(
        parse_java_version("java version \"1.8.0_392\""),
        Some(("1.8.0_392".into(), 8))
    );
    assert_eq!(
        parse_java_version("openjdk version \"26\" 2026-03-17"),
        Some(("26".into(), 26))
    );
    assert_eq!(parse_java_version("nothing here"), None);
}

#[test]
fn reads_the_gradle_wrapper() {
    let props = "distributionBase=GRADLE_USER_HOME\ndistributionUrl=https\\://services.gradle.org/distributions/gradle-8.10.2-bin.zip\n";
    assert_eq!(parse_gradle_wrapper(props), Some((8, 10)));
    assert_eq!(
        parse_gradle_wrapper("distributionUrl=https://x/gradle-9.0-all.zip"),
        Some((9, 0))
    );
}

#[test]
fn gradle_limits_java() {
    assert_eq!(max_java_for_gradle((8, 10)), 23);
    assert_eq!(max_java_for_gradle((8, 5)), 21);
    assert_eq!(max_java_for_gradle((9, 9)), 26);
    let g = (8, 10);
    assert!(incompatibility(&jdk(11, "x"), g)
        .unwrap()
        .contains("too old"));
    assert!(incompatibility(&jdk(26, "x"), g)
        .unwrap()
        .contains("too new"));
    assert!(incompatibility(&jdk(21, "x"), g).is_none());
}

#[test]
fn recommends_an_lts_preferring_android_studio() {
    let g = (8, 10);
    let list = vec![
        jdk(26, "Homebrew"),
        jdk(21, "Homebrew"),
        jdk(21, "Android Studio"),
        jdk(17, "System"),
        jdk(11, "System"),
    ];
    assert_eq!(recommend(&list, g).unwrap().source, "Android Studio");
    assert_eq!(
        recommend(&[jdk(26, "Homebrew"), jdk(17, "System")], g)
            .unwrap()
            .major,
        17
    );
    assert!(recommend(&[jdk(26, "Homebrew"), jdk(11, "System")], g).is_none());
}
