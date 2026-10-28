//! The project window: its steps and the job drawer.

use super::*;

#[component]
pub(crate) fn ProjectView(project_dir: PathBuf, on_close: EventHandler<()>) -> Element {
    let settings = use_context::<Signal<Settings>>();

    let dir = use_signal(|| project_dir.clone());
    let proj = use_signal(|| {
        let mut state = load_project_state(&project_dir);
        // Identity first: the profile match below needs the bundle ID.
        let identified = seed_identity(&mut state, &project_dir);
        if seed_release_fields(&mut state, &project_dir, &settings.peek()) | identified {
            save_project_state(&project_dir, &state);
        }
        state
    });
    let refresh = use_signal(|| 0u32);
    let creds = use_memo(move || {
        refresh.read();
        read_creds(&dir.read())
    });
    let artifact_paths = use_memo(move || {
        let p = proj.read();
        (
            p.ios_artifact_path.trim().to_string(),
            p.android_artifact_path.trim().to_string(),
        )
    });
    let artifacts = use_memo(move || {
        refresh.read();
        let (ios, android) = artifact_paths();
        scan_artifacts(&dir.read(), &ios, &android)
    });
    // Whether the built-in build scripts can build this project at all.
    let dioxus = use_memo(move || {
        refresh.read();
        is_dioxus_project(&dir.read())
    });
    // Two stages so typing in unrelated project fields doesn't re-run the
    // Keychain lookup: the inputs memo only changes when its values do.
    let signing_inputs = use_memo(move || {
        (
            settings.read().android_keystore_path.clone(),
            key_alias(&proj.read()),
        )
    });
    let android = use_memo(move || {
        refresh.read();
        let (keystore, alias) = signing_inputs();
        resolve_android_signing(&dir.read(), &keystore, &alias).0
    });
    let android_package = use_memo(move || proj.read().android_bundle_id.trim().to_string());
    let package_mismatches = use_memo(move || {
        refresh.read();
        let expected = android_package();
        if expected.is_empty() {
            Vec::new()
        } else {
            consistency::package_mismatches(&dir.read(), &expected)
        }
    });
    let repo = use_memo(move || {
        refresh.read();
        repo::repo_state(&dir.read())
    });
    let tracked_secrets = use_memo(move || {
        refresh.read();
        signing::tracked_secrets(&dir.read())
    });
    let backup = use_memo(move || {
        refresh.read();
        backup_state(&settings.read())
    });
    let profile = use_memo(move || {
        refresh.read();
        let path = proj.read().provisioning_profile.trim().to_string();
        (!path.is_empty())
            .then(|| read_profile(std::path::Path::new(&path)))
            .flatten()
    });
    let active_locale = use_signal(|| {
        proj.peek()
            .locales
            .first()
            .cloned()
            .unwrap_or_else(|| "en-US".to_string())
    });

    // What Claude changes through `shipscreen mcp` shows up here.
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let project = dir.peek().clone();
            if project_state_changed_elsewhere(&project) {
                let (mut proj, mut refresh) = (proj, refresh);
                proj.set(load_project_state(&project));
                refresh.with_mut(|n| *n += 1);
            }
        }
    });

    let ws = Ws {
        dir,
        proj,
        settings,
        step: use_signal(|| Step::App),
        scope: dioxus::core::current_scope_id(),
        refresh,
        creds,
        artifacts,
        dioxus,
        android,
        tracked_secrets,
        repo,
        package_mismatches,
        backup,
        profile,
        asc_test: use_signal(|| JobState::Idle),
        play_test: use_signal(|| JobState::Idle),
        cert_job: use_signal(|| JobState::Idle),
        profile_job: use_signal(|| JobState::Idle),
        aab_check: use_signal(Vec::new),
        ipa_check: use_signal(Vec::new),
        active_locale,
        gen_phase: use_signal(|| AppPhase::Idle),
        gen_log: use_signal(Vec::new),
        build_phase: use_signal(|| BuildPhase::Idle),
        build_log: use_signal(Vec::new),
        last_build: use_signal(String::new),
        ios_pub: use_signal(|| PublishPhase::Idle),
        ios_pub_log: use_signal(Vec::new),
        play_pub: use_signal(|| AndroidPublishPhase::Idle),
        play_pub_log: use_signal(Vec::new),
        ios_build_job: use_signal(|| JobState::Idle),
        ios_build_log: use_signal(Vec::new),
        ios_review_job: use_signal(|| JobState::Idle),
        ios_review_log: use_signal(Vec::new),
        play_release_job: use_signal(|| JobState::Idle),
        play_release_log: use_signal(Vec::new),
        drawer_open: use_signal(|| false),
        drawer_job: use_signal(|| JobKind::Screenshots),
    };
    use_context_provider(|| ws);
    // The JDK chosen in the Build step applies to everything ShipScreen runs.
    use_hook(move || java::set_preferred(&settings.peek().java_home));

    // Inspect each newly built bundle once (off the UI thread).
    use_effect(move || {
        let a = artifacts();
        let dir = ws.dir();
        let p = ws.proj.peek().clone();
        let keystore = ws.settings.peek().android_keystore_path.clone();
        let (mut aab_check, mut ipa_check) = (ws.aab_check, ws.ipa_check);
        spawn(async move {
            if let Some(aab) = a.aab.clone() {
                let (dir, p, keystore) = (dir.clone(), p.clone(), keystore.clone());
                let checks = tokio::task::spawn_blocking(move || {
                    let (signing, password) =
                        resolve_android_signing(&dir, &keystore, &key_alias(&p));
                    let expected = signing.keystore.zip(password).and_then(|(ks, pw)| {
                        signing::upload_key_fingerprint(&ks, &signing.alias, &pw).ok()
                    });
                    verify::verify_aab(
                        &aab.path,
                        &dir,
                        &p.project_slug,
                        &p.android_bundle_id,
                        expected.as_deref(),
                    )
                })
                .await
                .unwrap_or_default();
                aab_check.set(checks);
            } else {
                aab_check.set(Vec::new());
            }
            if let Some(ipa) = a.ipa.clone() {
                let bundle = p.ios_bundle_id.clone();
                let checks =
                    tokio::task::spawn_blocking(move || verify::verify_ipa(&ipa.path, &bundle))
                        .await
                        .unwrap_or_default();
                ipa_check.set(checks);
            } else {
                ipa_check.set(Vec::new());
            }
        });
    });

    // Open on the first step that still needs work, once.
    use_hook(move || {
        let steps = visible_steps(&proj.peek().platform_type);
        let first = steps
            .iter()
            .copied()
            .find(|s| ws.status(*s) != StepStatus::Done)
            .unwrap_or(Step::App);
        let mut step = ws.step;
        step.set(first);
    });

    let steps = visible_steps(&proj.read().platform_type);
    // A platform change can hide the current step (e.g. switching to Desktop).
    let current = {
        let s = *ws.step.read();
        if steps.contains(&s) {
            s
        } else {
            Step::App
        }
    };
    let idx = steps.iter().position(|s| *s == current).unwrap_or(0);
    let prev = idx.checked_sub(1).map(|i| steps[i]);
    let next = steps.get(idx + 1).copied();

    let proj_name = {
        let name = proj.read().app_name.trim().to_string();
        if name.is_empty() {
            project_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        } else {
            name
        }
    };
    let dir_display = project_dir.to_string_lossy().to_string();

    rsx! {
        div { class: "ws",
            // ---- Sidebar ----
            nav { class: "ws-side", aria_label: "Project steps",
                div { class: "ws-side-head",
                    button {
                        class: "btn btn-icon btn-back",
                        onclick: move |_| on_close.call(()),
                        title: "Back to projects",
                        aria_label: "Back to projects",
                        {icon_chevron_left()}
                    }
                    div { class: "ws-proj",
                        p { class: "ws-proj-name", "{proj_name}" }
                        // LRM keeps the leading "/" in place under the rtl clipping (see .ws-proj-path)
                        p { class: "ws-proj-path", title: "{dir_display}", "\u{200E}{dir_display}" }
                    }
                }

                div { class: "ws-nav",
                    for (i, s) in steps.iter().copied().enumerate() {
                        if i == 0 || steps[i - 1].group() != s.group() {
                            p { class: "ws-group-label", "{s.group()}" }
                        }
                        StepLink { step: s, number: i + 1, active: s == current }
                    }
                }

            }

            // ---- Step screen ----
            main { class: "ws-main",
                div { class: "ws-content",
                    header { class: "step-head",
                        p { class: "step-eyebrow", "Step {idx + 1} of {steps.len()} · {current.group()}" }
                        h1 { "{current.heading()}" }
                        p { class: "step-blurb", "{current.blurb()}" }
                    }

                    match current {
                        Step::App => rsx! { steps::AppStep {} },
                        Step::Accounts => rsx! { accounts::AccountsStep {} },
                        Step::Version => rsx! { steps::VersionStep {} },
                        Step::Build => rsx! { steps::BuildStep {} },
                        Step::Languages => rsx! { steps::LanguagesStep {} },
                        Step::Screenshots => rsx! { steps::ScreenshotsStep {} },
                        Step::Metadata => rsx! { steps::MetadataStep {} },
                        Step::Submit => rsx! { steps::SubmitStep {} },
                    }

                    div { class: "step-nav",
                        if let Some(p) = prev {
                            button {
                                class: "btn",
                                onclick: move |_| ws.go(p),
                                {icon_chevron_left()}
                                "{p.label()}"
                            }
                        }
                        if let Some(n) = next {
                            button {
                                class: "btn btn-primary step-nav-next",
                                onclick: move |_| ws.go(n),
                                "Continue to {n.label()}"
                                {icon_chevron_right()}
                            }
                        }
                    }
                }

                JobDrawer {}
            }
        }
    }
}

#[component]
pub(super) fn StepLink(step: Step, number: usize, active: bool) -> Element {
    let ws = use_context::<Ws>();
    let status = ws.status(step);
    let (mark_class, status_label) = match status {
        StepStatus::Done => ("ws-step-mark mark-done", "done"),
        StepStatus::Attention => ("ws-step-mark mark-attn", "needs attention"),
        StepStatus::Running => ("ws-step-mark mark-run", "running"),
        StepStatus::Todo => ("ws-step-mark", "to do"),
    };
    rsx! {
        button {
            class: if active { "ws-step ws-step-active" } else { "ws-step" },
            aria_current: if active { "step" },
            title: "{step.label()} — {status_label}",
            onclick: move |_| ws.go(step),
            span { class: "{mark_class}",
                match status {
                    StepStatus::Done => rsx! { {icon_check()} },
                    StepStatus::Attention => rsx! { "!" },
                    StepStatus::Running => rsx! { span { class: "spinner spinner-dark" } },
                    StepStatus::Todo => rsx! { "{number}" },
                }
            }
            span { class: "ws-step-label", "{step.label()}" }
        }
    }
}

#[component]
pub(super) fn JobDrawer() -> Element {
    let ws = use_context::<Ws>();
    let mut drawer_open = ws.drawer_open;
    let mut drawer_job = ws.drawer_job;

    let jobs: Vec<(JobKind, JobState)> = JobKind::ALL
        .into_iter()
        .map(|k| (k, ws.job_state(k)))
        .filter(|(_, s)| *s != JobState::Idle)
        .collect();

    let selected = *drawer_job.read();
    let log = ws.job_log(selected);

    // Keep the open log pinned to its newest line.
    use_effect(move || {
        let _ = log.read().len();
        let _ = drawer_open.read();
        spawn(async move {
            document::eval(
                "setTimeout(() => { for (const el of document.querySelectorAll('.drawer-log')) el.scrollTop = el.scrollHeight; }, 50);",
            )
            .await
            .ok();
        });
    });

    if jobs.is_empty() {
        return rsx! {};
    }
    let open = *drawer_open.read();

    rsx! {
        section { class: if open { "drawer drawer-open" } else { "drawer" }, aria_label: "Jobs",
            div { class: "drawer-bar",
                span { class: "drawer-title", "Jobs" }
                div { class: "drawer-chips",
                    for (kind, state) in jobs.iter().cloned() {
                        {
                            let (dot, text) = match &state {
                                JobState::Running(t) => ("drawer-dot dot-run", t.clone()),
                                JobState::Ok(t) => ("drawer-dot dot-ok", t.clone()),
                                JobState::Failed(_) => ("drawer-dot dot-err", "Failed".to_string()),
                                JobState::Idle => ("drawer-dot", String::new()),
                            };
                            let is_sel = open && kind == selected;
                            rsx! {
                                button {
                                    class: if is_sel { "drawer-chip drawer-chip-active" } else { "drawer-chip" },
                                    onclick: move |_| {
                                        if open && kind == selected {
                                            drawer_open.set(false);
                                        } else {
                                            drawer_job.set(kind);
                                            drawer_open.set(true);
                                        }
                                    },
                                    span { class: "{dot}" }
                                    strong { "{kind.label()}" }
                                    span { class: "drawer-chip-text", "{text}" }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "btn btn-sm drawer-toggle",
                    onclick: move |_| drawer_open.toggle(),
                    if open { "Hide log" } else { "Show log" }
                }
            }
            if open {
                div { class: "drawer-body",
                    if let JobState::Failed(msg) = ws.job_state(selected) {
                        p { class: "drawer-error", role: "alert",
                            {icon_alert()}
                            "{msg}"
                        }
                    }
                    if log.read().is_empty() {
                        p { class: "output-empty", "No output yet." }
                    } else {
                        div { class: "log-scroll drawer-log",
                            for line in log.read().iter() {
                                p { class: log_class(line), "{line}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
