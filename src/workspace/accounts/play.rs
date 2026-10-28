//! The Google Play account and its service key.

use super::*;

#[component]
pub(crate) fn PlayCard() -> Element {
    let ws = use_context::<Ws>();
    let creds = ws.creds.read().clone();
    rsx! {
        div { class: "card",
            h2 { "Google Play" }
            ul { class: "check-list",
                for c in creds.play.iter().cloned() {
                    {check_row(ws, c.ok, c.label, c.detail, None)}
                }
            }
            if creds.play_ok() {
                details { class: "inline-form",
                    summary { "Use a different key" }
                    PlayKeySetup {}
                }
            } else {
                PlayKeySetup {}
            }
            {connection_test(ws.play_test.read().clone(), move |_| jobs::test_google_play(ws))}
        }
    }
}

/// Get the service-account key into the project: reuse another project's,
/// or walk through creating one, picking up the downloaded JSON.
#[component]
pub(crate) fn PlayKeySetup() -> Element {
    let ws = use_context::<Ws>();
    let mut others = use_signal(Vec::<playkey::PlayKey>::new);
    let mut found = use_signal(Vec::<playkey::PlayKey>::new);
    let mut chosen = use_signal(|| Option::<playkey::PlayKey>::None);
    let mut state = use_signal(|| JobState::Idle);

    use_effect(move || {
        ws.refresh.read();
        let projects = ws.settings.peek().recent_projects.clone();
        let (dir, keys) = (ws.dir(), keys_dir(&ws.settings.peek()));
        spawn(async move {
            let (o, f) = tokio::task::spawn_blocking(move || {
                (
                    playkey::from_other_projects(&projects, &dir),
                    playkey::find_keys(&keys),
                )
            })
            .await
            .unwrap_or_default();
            // A single key on the Mac is almost certainly the one to use.
            if chosen.peek().is_none() && f.len() == 1 {
                chosen.set(Some(f[0].clone()));
            }
            others.set(o);
            found.set(f);
        });
    });

    let mut save = move |key: playkey::PlayKey| {
        let dir = ws.dir();
        let keys = keys_dir(&ws.settings.peek());
        let result = playkey::keep_in_keys_dir(&key.path, &keys).and_then(|kept| {
            envfile::set_values(&dir, &[(playkey::KEY_VAR, &kept.to_string_lossy())]).map(|_| kept)
        });
        match result {
            Ok(kept) => {
                state.set(JobState::Ok(format!(
                    "Saved in the project's .env · key kept at {}",
                    kept.display()
                )));
                let mut refresh = ws.refresh;
                refresh.with_mut(|n| *n += 1);
                jobs::test_google_play(ws);
            }
            Err(e) => state.set(JobState::Failed(e)),
        }
    };

    rsx! {
        if !others.read().is_empty() {
            p { class: "settings-hint", "One service account can release every app of your developer account — this Mac already has one:" }
            for k in others.read().iter().cloned() {
                div { class: "install-option",
                    div { class: "check-text",
                        span { class: "check-label", "{k.client_email}" }
                        span { class: "check-detail", "Used by {k.from} · {k.path.display()}" }
                    }
                    div { class: "install-actions",
                        button { class: "btn btn-sm", onclick: move |_| save(k.clone()), "Use this key" }
                    }
                }
            }
            p { class: "settings-hint",
                "If the test then says it can't edit this app, give that account access to it in "
                a { href: playkey::PLAY_USERS_PAGE, target: "_blank", "Play Console → Users and permissions" }
                "."
            }
        }
        details { class: "inline-form", open: others.read().is_empty(),
            summary { if others.read().is_empty() { "Set up a key" } else { "Or create a new key" } }
            ol { class: "lost-steps",
                li {
                    "In Google Cloud, pick (or create) a project and "
                    a { href: playkey::API_PAGE, target: "_blank", "enable the Google Play Android Developer API" }
                    "."
                }
                li {
                    "Open "
                    a { href: playkey::ACCOUNTS_PAGE, target: "_blank", "Service accounts" }
                    ", create one (no roles needed), then Keys → Add key → Create new key → JSON. The file downloads once; ShipScreen keeps a copy in your keys folder."
                }
                li {
                    "In "
                    a { href: playkey::PLAY_USERS_PAGE, target: "_blank", "Play Console → Users and permissions" }
                    ", invite the service account's email (shown below once the key is chosen) and give it access to this app with permission to manage releases and the store listing."
                }
            }
            div { class: "build-config-field",
                label { class: "build-config-label", "Service-account key (JSON)" }
                if found.read().len() > 1 {
                    select {
                        class: "text-input",
                        onchange: move |e: Event<FormData>| {
                            let v = e.value();
                            let k = found.peek().iter().find(|k| k.path.to_string_lossy() == v).cloned();
                            chosen.set(k);
                        },
                        option { value: "", selected: chosen().is_none(), disabled: true, "— keys on this Mac —" }
                        for k in found.read().iter() {
                            option { value: "{k.path.display()}", selected: chosen().as_ref().is_some_and(|c| c.path == k.path), "{k.client_email} · {k.path.display()}" }
                        }
                    }
                } else if let Some(k) = chosen() {
                    p { class: "settings-hint", "{k.path.display()}" }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(path) = pick_file("Choose the service-account key", ("JSON key", &["json"])).await {
                                match playkey::read_key(&path) {
                                    Some(k) => { chosen.set(Some(k)); state.set(JobState::Idle); }
                                    None => state.set(JobState::Failed(format!("{} isn't a Google service-account key", path.display()))),
                                }
                            }
                        });
                    },
                    "Choose JSON…"
                }
            }
            if let Some(k) = chosen() {
                div { class: "install-option",
                    div { class: "check-text",
                        span { class: "check-label", "Invite this email in Play Console" }
                        span { class: "check-detail", "{k.client_email}" }
                    }
                    div { class: "install-actions", {copy_button(k.client_email.clone(), state)} }
                }
            }
            div { class: "conn-test",
                button {
                    class: "btn btn-sm",
                    disabled: chosen().is_none(),
                    onclick: move |_| {
                        if let Some(k) = chosen() {
                            save(k);
                        }
                    },
                    "Save and test"
                }
                if chosen().is_none() {
                    span { class: "settings-hint", "Choose the downloaded JSON key" }
                }
            }
            p { class: "settings-hint",
                a { href: playkey::HELP_PAGE, target: "_blank", "Google's guide to API access" }
            }
        }
        div { class: "publish-status-row", {job_status_line(state(), "")} }
    }
}
