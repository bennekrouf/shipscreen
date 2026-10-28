//! The checks a build needs first: the doctor, and Java.

use super::*;

/// Build prerequisites, checked when the step opens and after every fix.
#[component]
pub(crate) fn DoctorCard() -> Element {
    let ws = use_context::<Ws>();
    let mut items = use_signal(|| Option::<Vec<doctor::DoctorItem>>::None);
    let mut fix_state = use_signal(|| JobState::Idle);
    let mut rerun = use_signal(|| 0u32);

    use_effect(move || {
        rerun.read();
        let dir = ws.dir();
        let platform = ws.proj.peek().platform_type.clone();
        let identity = ws.settings.peek().apple_identity.clone();
        spawn(async move {
            let found =
                tokio::task::spawn_blocking(move || doctor::run(&dir, &platform, &identity))
                    .await
                    .unwrap_or_default();
            items.set(Some(found));
        });
    });

    let Some(list) = items() else {
        return rsx! {
            div { class: "card build-status-card",
                div { class: "publish-status-row", span { class: "spinner spinner-dark" } span { class: "publish-status publish-running", "Checking build tools…" } }
            }
        };
    };
    let problems = list.iter().filter(|i| !i.ok).count();

    rsx! {
        details { class: "card doctor", open: problems > 0,
            summary {
                if problems == 0 {
                    span { class: "publish-status publish-success", {icon_check()} "Build tools ready" }
                } else {
                    span { class: "publish-status publish-error", {icon_alert()} "{problems} thing(s) to fix before building" }
                }
            }
            for section in ["Tools", "Android", "iOS"] {
                if list.iter().any(|i| i.section == section) {
                    p { class: "export-section-label", "{section}" }
                    ul { class: "check-list",
                        for item in list.iter().filter(|i| i.section == section).cloned() {
                            li { class: if item.ok { "check-row check-ok" } else { "check-row check-bad" },
                                span { class: "check-mark", if item.ok { {icon_check()} } else { {icon_alert()} } }
                                div { class: "check-text",
                                    div { class: "check-head",
                                        span { class: "check-label", "{item.label}" }
                                        if let Some(fix) = item.fix.clone() {
                                            button {
                                                class: "check-fix",
                                                disabled: matches!(fix_state(), JobState::Running(_)),
                                                onclick: move |_| {
                                                    if let doctor::Fix::GoTo(step) = fix {
                                                        ws.go(step);
                                                        return;
                                                    }
                                                    if let doctor::Fix::UseJava(home, major) = &fix {
                                                        use_java(ws, home);
                                                        fix_state.set(JobState::Ok(format!("Builds now use Java {major}")));
                                                        rerun.with_mut(|n| *n += 1);
                                                        return;
                                                    }
                                                    let (fix, dir) = (fix.clone(), ws.dir());
                                                    fix_state.set(JobState::Running(format!("{}…", fix.label())));
                                                    spawn(async move {
                                                        let r = tokio::task::spawn_blocking(move || doctor::apply(&dir, &fix)).await.unwrap_or_else(|e| Err(e.to_string()));
                                                        fix_state.set(match r {
                                                            Ok(m) => JobState::Ok(m),
                                                            Err(e) => JobState::Failed(e),
                                                        });
                                                        rerun.with_mut(|n| *n += 1);
                                                    });
                                                },
                                                "{fix.label()}"
                                            }
                                        }
                                    }
                                    span { class: "check-detail", "{item.detail}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "conn-test",
                button { class: "btn btn-sm", onclick: move |_| rerun.with_mut(|n| *n += 1), "Check again" }
                div { class: "publish-status-row", {job_status_line(fix_state(), "")} }
            }
        }
    }
}

/// Which Java ShipScreen builds with — every JDK on the machine, checked
/// against the project's Gradle version.
#[component]
pub(crate) fn JavaCard() -> Element {
    let ws = use_context::<Ws>();
    let mut settings = ws.settings;
    let mut jdks = use_signal(|| Option::<Vec<java::Jdk>>::None);
    let mut rescan = use_signal(|| 0u32);
    let (gradle, detected) = java::project_gradle(&ws.dir());

    use_effect(move || {
        if rescan() > 0 {
            java::refresh();
        }
        spawn(async move {
            jdks.set(Some(
                tokio::task::spawn_blocking(java::installed)
                    .await
                    .unwrap_or_default(),
            ));
        });
    });

    // What this machine can install Java with (looked up once, off the UI thread).
    let mut installers = use_signal(|| Option::<java::Installers>::None);
    use_effect(move || {
        if jdks().is_some() && installers.peek().is_none() {
            spawn(async move {
                installers.set(Some(
                    tokio::task::spawn_blocking(java::detect_installers)
                        .await
                        .unwrap_or_default(),
                ));
            });
        }
    });
    let install_log = use_signal(Vec::<String>::new);
    let mut install_state = use_signal(|| JobState::Idle);

    let chosen = settings.read().java_home.clone();
    let list = jdks().unwrap_or_default();
    let recommended = java::recommend(&list, gradle);
    let major = java::install_major(gradle);
    let options = installers()
        .map(|i| java::install_options(java::current_os(), major, &i))
        .unwrap_or_default();
    // What builds will really run on: the pick, unless Gradle can't run on it.
    let chosen_jdk = list
        .iter()
        .find(|j| !chosen.is_empty() && j.home.to_string_lossy() == chosen)
        .cloned();
    let chosen_problem = chosen_jdk
        .as_ref()
        .and_then(|j| java::incompatibility(j, gradle));
    let env_jdk = list.iter().find(|j| j.source == "JAVA_HOME").cloned();
    let (current, _) = java::pick_for_build(chosen_jdk, env_jdk, &list, gradle);
    let current = current.filter(|_| jdks().is_some());
    let label = |j: &java::Jdk| {
        let problem = java::incompatibility(j, gradle)
            .map(|p| format!(" — {p}"))
            .unwrap_or_default();
        format!(
            "Java {} · {} · {}{problem}",
            j.version,
            j.source,
            j.home.display()
        )
    };

    rsx! {
        details { class: "card doctor",
            summary {
                span { class: "publish-status publish-idle",
                    "Java for builds: "
                    match &current {
                        Some(j) => rsx! { "{j.version} ({j.source})" },
                        None if jdks().is_none() => rsx! { "checking…" },
                        None => rsx! { "none found" },
                    }
                }
            }
            p { class: "settings-hint",
                "ShipScreen runs keytool and Gradle with this Java — your terminal and system settings are left alone. "
                if detected { "The project builds with Gradle {gradle.0}.{gradle.1}." } else { "Gradle {gradle.0}.{gradle.1} assumed until the first Android build." }
            }
            if jdks().is_none() {
                p { class: "settings-hint", span { class: "spinner spinner-dark" } " Looking for installed Java versions…" }
            } else if list.is_empty() {
                p { class: "settings-hint hint-error",
                    "No Java installed. Android Studio includes one; otherwise install Java 21 (macOS: brew install --cask temurin@21 · Linux: sudo apt install openjdk-21-jdk · Windows: winget install EclipseAdoptium.Temurin.21.JDK)."
                }
            } else {
                select {
                    class: "text-input",
                    onchange: move |e: Event<FormData>| {
                        settings.write().java_home = e.value();
                        save_settings(&settings.peek());
                        java::set_preferred(&e.value());
                    },
                    option {
                        value: "",
                        selected: chosen.is_empty(),
                        match &recommended {
                            Some(r) => format!("Automatic — Java {} ({})", r.version, r.source),
                            None => "Automatic — no compatible Java installed".to_string(),
                        }
                    }
                    for j in list.iter() {
                        option { value: "{j.home.display()}", selected: chosen == j.home.to_string_lossy(), "{label(j)}" }
                    }
                }
            }
            if let (Some(problem), Some(fix)) = (chosen_problem.clone(), recommended.clone()) {
                div { class: "install-option",
                    div { class: "check-text",
                        span { class: "check-label hint-error", "The Java you picked can't build this project" }
                        span { class: "check-detail", "{problem}. Builds use Java {fix.version} ({fix.source}) instead until you switch." }
                    }
                    div { class: "install-actions",
                        button { class: "btn btn-sm", onclick: move |_| use_java(ws, ""), "Use Java {fix.major}" }
                    }
                }
            }
            if !options.is_empty() {
                details { class: "inline-form", open: jdks().is_some() && recommended.is_none(),
                    summary {
                        if recommended.is_none() { "Install Java {major}" } else { "Install another Java" }
                    }
                    for opt in options.iter().cloned() {
                        div { class: "install-option",
                            div { class: "check-text",
                                span { class: "check-label", "{opt.title}" }
                                span { class: "check-detail", "{opt.note}" }
                                if !opt.command.is_empty() {
                                    code { class: "install-command", "{opt.command}" }
                                }
                            }
                            div { class: "install-actions",
                                match opt.runner.clone() {
                                    java::Runner::InApp { program, args } => rsx! {
                                        button {
                                            class: "btn btn-sm",
                                            disabled: matches!(install_state(), JobState::Running(_)),
                                            onclick: move |_| {
                                                let (program, args) = (program.clone(), args.clone());
                                                let mut log = install_log;
                                                log.set(Vec::new());
                                                install_state.set(JobState::Running(format!("Installing Java {major}…")));
                                                spawn(async move {
                                                    let mut cmd = std::process::Command::new(&program);
                                                    cmd.args(&args);
                                                    match jobs::stream_command(cmd, "installer", log).await {
                                                        Ok(()) => {
                                                            install_state.set(JobState::Ok(format!("Java {major} installed")));
                                                            jdks.set(None);
                                                            rescan.with_mut(|n| *n += 1);
                                                        }
                                                        Err(e) => install_state.set(JobState::Failed(e)),
                                                    }
                                                });
                                            },
                                            "Install"
                                        }
                                    },
                                    java::Runner::Terminal => rsx! {
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let command = opt.command.clone();
                                                move |_| {
                                                    install_state.set(match java::open_in_terminal(&command) {
                                                        Ok(()) => JobState::Ok("Finish in the terminal, then click Scan again".into()),
                                                        Err(e) => JobState::Failed(e),
                                                    });
                                                }
                                            },
                                            "Open in Terminal"
                                        }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let command = opt.command.clone();
                                                move |_| {
                                                    let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&command).unwrap_or_default());
                                                    document::eval(&js);
                                                    install_state.set(JobState::Ok("Command copied".into()));
                                                }
                                            },
                                            "Copy command"
                                        }
                                    },
                                    java::Runner::Browser(url) => rsx! {
                                        a { class: "btn btn-sm", href: "{url}", target: "_blank", "Download" }
                                    },
                                }
                            }
                        }
                    }
                    if !install_log.read().is_empty() {
                        div { class: "log-scroll drawer-log install-log",
                            for line in install_log.read().iter() {
                                p { class: "log-line", "{line}" }
                            }
                        }
                    }
                    div { class: "publish-status-row", {job_status_line(install_state(), "")} }
                }
            }
            if let Some(j) = current.clone() {
                TerminalJava { jdk: j, gradle }
            }
            div { class: "conn-test",
                button { class: "btn btn-sm", onclick: move |_| { jdks.set(None); rescan.with_mut(|n| *n += 1); }, "Scan again" }
            }
        }
    }
}

/// Opt-in: the same Java for the developer's own terminal, previewed,
/// reversible, and proven by asking a fresh login shell what it gets.
#[component]
pub(crate) fn TerminalJava(jdk: java::Jdk, gradle: (u32, u32)) -> Element {
    let ws = use_context::<Ws>();
    let target = use_hook(shellenv::detect_target);
    let mut refresh = use_signal(|| 0u32);
    let status = use_memo({
        let target = target.clone();
        move || {
            refresh();
            shellenv::status(&target)
        }
    });
    let mut state = use_signal(|| JobState::Idle);
    let mut proof = use_signal(Vec::<(String, String)>::new);

    let home = jdk.home.display().to_string();
    let st = status();
    let up_to_date = st.installed.as_deref() == Some(home.as_str());
    let preview = match &target {
        shellenv::Target::Profile { flavor, .. } => shellenv::block(*flavor, &jdk.home),
        shellenv::Target::WindowsUser => format!("JAVA_HOME = {home}\nPath += %JAVA_HOME%\\bin"),
    };
    let where_ = target.describe();
    // What the last check found wrong with the terminal's Java, if anything.
    let terminal_problem = proof.read().iter().find(|(label, _)| label == "java").and_then(|(_, line)| {
        if line.contains("not found") {
            return Some(format!("New terminals have no java — {} to fix it", if up_to_date { "open a new window" } else { "add it above" }));
        }
        let (version, major) = java::parse_java_version(line)?;
        let j = java::Jdk { home: Default::default(), major, version, source: "terminal" };
        java::incompatibility(&j, gradle).map(|p| {
            if up_to_date {
                format!("New terminals run Java {major}: {p}. Something later in your shell setup overrides ShipScreen' lines.")
            } else {
                format!("New terminals run Java {major}: {p}. Click \"{}\" to switch them to Java {}.", if st.installed.is_some() { "Switch to this Java" } else { "Add to my terminal" }, jdk.major)
            }
        })
    });

    rsx! {
        details { class: "inline-form",
            summary {
                "Use this Java in my terminal too"
                match &st.installed {
                    Some(h) if *h == home => rsx! { " · on" },
                    Some(_) => rsx! { " · set to another Java" },
                    None => rsx! {},
                }
            }
            p { class: "settings-hint",
                "Optional — ShipScreen doesn't need it. For running Gradle or other Java tools yourself. "
                if up_to_date { "Already in {where_}." } else { "This goes at the end of {where_}:" }
            }
            if !up_to_date {
                code { class: "install-command", "{preview}" }
            }
            if !st.others.is_empty() {
                p { class: "settings-hint",
                    "{where_} also sets JAVA_HOME on line {st.others.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(\", \")}. ShipScreen' lines come after, so they win; you can delete the old ones."
                }
            }
            div { class: "install-actions",
                if !up_to_date {
                    button {
                        class: "btn btn-sm",
                        onclick: {
                            let (target, home) = (target.clone(), jdk.home.clone());
                            move |_| {
                                state.set(match shellenv::apply(&target, &home) {
                                    Ok(m) => JobState::Ok(m),
                                    Err(e) => JobState::Failed(e),
                                });
                                proof.set(Vec::new());
                                refresh.with_mut(|n| *n += 1);
                            }
                        },
                        if st.installed.is_some() { "Switch to this Java" } else { "Add to my terminal" }
                    }
                }
                if st.installed.is_some() {
                    button {
                        class: "btn btn-sm",
                        onclick: {
                            let target = target.clone();
                            move |_| {
                                state.set(match shellenv::remove(&target) {
                                    Ok(m) => JobState::Ok(m),
                                    Err(e) => JobState::Failed(e),
                                });
                                proof.set(Vec::new());
                                refresh.with_mut(|n| *n += 1);
                            }
                        },
                        "Remove"
                    }
                }
                button {
                    class: "btn btn-sm",
                    disabled: matches!(state(), JobState::Running(_)),
                    onclick: {
                        let target = target.clone();
                        move |_| {
                            let target = target.clone();
                            let gradle = java::gradle_project_dir(&ws.dir());
                            state.set(JobState::Running(if gradle.is_some() {
                                "Asking a new terminal and Gradle which Java they use…".into()
                            } else {
                                "Asking a new terminal which Java it uses…".into()
                            }));
                            spawn(async move {
                                let rows = tokio::task::spawn_blocking(move || shellenv::prove(&target, gradle.as_deref()))
                                    .await
                                    .unwrap_or_default();
                                proof.set(rows);
                                state.set(JobState::Idle);
                            });
                        }
                    },
                    "Check my terminal"
                }
            }
            if !proof.read().is_empty() {
                div { class: "build-checks",
                    for (label, value) in proof.read().iter() {
                        div { class: "check-text",
                            span { class: "check-label", "{label}" }
                            span { class: "check-detail", "{value}" }
                        }
                    }
                }
                match &terminal_problem {
                    Some(p) => rsx! { p { class: "settings-hint hint-error", {icon_alert()} " {p}" } },
                    None => rsx! { p { class: "settings-hint", {icon_check()} " Your terminal's Java works with Gradle {gradle.0}.{gradle.1}." } },
                }
            }
            div { class: "publish-status-row", {job_status_line(state(), "")} }
        }
    }
}

/// A build that failed on a Java mismatch: say so in plain words and offer
/// to build again with a Java that fits.
#[component]
pub(crate) fn BuildJavaFix() -> Element {
    let ws = use_context::<Ws>();
    let failed = matches!(*ws.build_phase.read(), BuildPhase::Error(_));
    let issue = use_memo(move || {
        if !matches!(*ws.build_phase.read(), BuildPhase::Error(_)) {
            return None;
        }
        java::diagnose_build_log(&ws.build_log.read())
    });
    let Some(issue) = issue().filter(|_| failed) else {
        return rsx! {};
    };
    let (gradle, _) = java::project_gradle(&ws.dir());
    let fix = java::remedy(&java::installed(), gradle, &issue);
    let script = ws.last_build.read().clone();

    rsx! {
        div { class: "card build-status-card",
            div { class: "check-text",
                span { class: "check-label hint-error", {icon_alert()} " {issue.explain(gradle)}" }
                match &fix {
                    Some(j) => rsx! { span { class: "check-detail", "Java {j.version} ({j.source}) is installed and works with Gradle {gradle.0}.{gradle.1}." } },
                    None => rsx! { span { class: "check-detail", "No installed Java fits — install Java {java::install_major(gradle)} in the Java card above, then build again." } },
                }
            }
            if let Some(j) = fix {
                button {
                    class: "btn btn-sm",
                    disabled: script.is_empty(),
                    onclick: move |_| {
                        use_java(ws, &j.home.to_string_lossy());
                        jobs::run_build(ws, script.clone());
                    },
                    "Build again with Java {j.major}"
                }
            }
        }
    }
}
