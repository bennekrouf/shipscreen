//! The open project's workspace: its state, and the jobs it runs.

use super::*;

#[derive(Clone, Copy)]
pub(crate) struct Ws {
    pub(super) dir: Signal<PathBuf>,
    pub(super) proj: Signal<ProjectState>,
    pub(super) settings: Signal<Settings>,
    pub(super) step: Signal<Step>,
    /// Scope that owns the jobs — the workspace itself, so leaving a step
    /// doesn't cancel what it started.
    pub(super) scope: ScopeId,
    /// Bumped to re-read credentials and artifacts from disk.
    pub(super) refresh: Signal<u32>,
    pub(super) creds: Memo<Creds>,
    pub(super) artifacts: Memo<Artifacts>,
    /// Whether the built-in build scripts can build this project at all
    pub(super) dioxus: Memo<bool>,
    /// Upload key source for this project (no secret inside)
    pub(super) android: Memo<AndroidSigning>,
    /// Secret files git tracks in this project
    pub(super) tracked_secrets: Memo<Vec<String>>,
    /// Is the source code pushed somewhere other than this Mac?
    pub(super) repo: Memo<repo::RepoState>,
    /// References to a different Android package in the code or .env
    pub(super) package_mismatches: Memo<Vec<consistency::PackageRef>>,
    pub(super) backup: Memo<BackupState>,
    /// The project's provisioning profile, parsed (None if unset/unreadable).
    pub(super) profile: Memo<Option<ProfileInfo>>,
    /// Results of the Accounts step's "Test connection" buttons.
    pub(super) asc_test: Signal<JobState>,
    pub(super) play_test: Signal<JobState>,
    /// Accounts step: certificate creation and profile repair
    pub(super) cert_job: Signal<JobState>,
    pub(super) profile_job: Signal<JobState>,
    /// What the newest AAB / IPA actually contain (re-checked per build)
    pub(super) aab_check: Signal<Vec<CredCheck>>,
    pub(super) ipa_check: Signal<Vec<CredCheck>>,
    /// Language currently being edited in the Languages step.
    pub(super) active_locale: Signal<String>,

    pub(super) gen_phase: Signal<AppPhase>,
    pub(super) gen_log: Signal<Vec<String>>,
    pub(super) build_phase: Signal<BuildPhase>,
    pub(super) build_log: Signal<Vec<String>>,
    /// The build script last started, to run it again after a fix
    pub(super) last_build: Signal<String>,
    pub(super) ios_pub: Signal<PublishPhase>,
    pub(super) ios_pub_log: Signal<Vec<String>>,
    pub(super) play_pub: Signal<AndroidPublishPhase>,
    pub(super) play_pub_log: Signal<Vec<String>>,
    pub(super) ios_build_job: Signal<JobState>,
    pub(super) ios_build_log: Signal<Vec<String>>,
    pub(super) ios_review_job: Signal<JobState>,
    pub(super) ios_review_log: Signal<Vec<String>>,
    pub(super) play_release_job: Signal<JobState>,
    pub(super) play_release_log: Signal<Vec<String>>,

    pub(super) drawer_open: Signal<bool>,
    pub(super) drawer_job: Signal<JobKind>,
}

impl Ws {
    pub(super) fn dir(&self) -> PathBuf {
        self.dir.read().clone()
    }

    /// Mutate the project state and persist it.
    pub(super) fn update(mut self, f: impl FnOnce(&mut ProjectState)) {
        let dir = self.dir();
        let mut p = self.proj.write();
        f(&mut p);
        save_project_state(&dir, &p);
    }

    pub(super) fn spawn(&self, fut: impl std::future::Future<Output = ()> + 'static) {
        dioxus::core::Runtime::current().spawn(self.scope, fut);
    }

    pub(super) fn go(mut self, step: Step) {
        self.step.set(step);
        document::eval("window.scrollTo(0, 0);");
    }

    pub(super) fn show_job(mut self, kind: JobKind) {
        self.drawer_job.set(kind);
        self.drawer_open.set(true);
    }

    pub(super) fn job_state(&self, kind: JobKind) -> JobState {
        match kind {
            JobKind::Screenshots => match &*self.gen_phase.read() {
                AppPhase::Idle => JobState::Idle,
                AppPhase::GeneratingAi | AppPhase::GeneratingManual | AppPhase::Resizing => {
                    JobState::Running("Generating…".into())
                }
                AppPhase::Done => JobState::Ok("Generated".into()),
                AppPhase::Error(e) => JobState::Failed(e.clone()),
            },
            JobKind::Build => match &*self.build_phase.read() {
                BuildPhase::Idle => JobState::Idle,
                BuildPhase::Running(name) => JobState::Running(format!("Running {name}…")),
                BuildPhase::Success(name) => JobState::Ok(format!("{name} finished")),
                BuildPhase::Error(e) => JobState::Failed(e.clone()),
            },
            JobKind::AppStore => match &*self.ios_pub.read() {
                PublishPhase::Idle => JobState::Idle,
                PublishPhase::Running => JobState::Running("Uploading…".into()),
                PublishPhase::Success => JobState::Ok("Uploaded".into()),
                PublishPhase::Error(e) => JobState::Failed(e.clone()),
            },
            JobKind::GooglePlay => match &*self.play_pub.read() {
                AndroidPublishPhase::Idle => JobState::Idle,
                AndroidPublishPhase::Running => JobState::Running("Uploading…".into()),
                AndroidPublishPhase::Success => JobState::Ok("Uploaded".into()),
                AndroidPublishPhase::Error(e) => JobState::Failed(e.clone()),
            },
            JobKind::AppStoreBuild => self.ios_build_job.read().clone(),
            JobKind::AppStoreReview => self.ios_review_job.read().clone(),
            JobKind::PlayRelease => self.play_release_job.read().clone(),
        }
    }

    pub(super) fn job_log(&self, kind: JobKind) -> Signal<Vec<String>> {
        match kind {
            JobKind::Screenshots => self.gen_log,
            JobKind::Build => self.build_log,
            JobKind::AppStore => self.ios_pub_log,
            JobKind::GooglePlay => self.play_pub_log,
            JobKind::AppStoreBuild => self.ios_build_log,
            JobKind::AppStoreReview => self.ios_review_log,
            JobKind::PlayRelease => self.play_release_log,
        }
    }

    pub(super) fn status(&self, step: Step) -> StepStatus {
        let p = self.proj.read();
        let plat = &p.platform_type;
        match step {
            Step::App => {
                let ids_ok = (!plat.has_ios() || !p.ios_bundle_id.trim().is_empty())
                    && (!plat.has_android() || !p.android_bundle_id.trim().is_empty());
                if !p.app_name.trim().is_empty() && !p.project_slug.trim().is_empty() && ids_ok {
                    if plat.has_android() && !self.package_mismatches.read().is_empty() {
                        return StepStatus::Attention;
                    }
                    if !self.repo.read().safe() {
                        return StepStatus::Attention;
                    }
                    StepStatus::Done
                } else {
                    StepStatus::Todo
                }
            }
            Step::Accounts => {
                let s = self.settings.read();
                let creds = self.creds.read();
                let mut checks = Vec::new();
                let untested_or_ok = |t: &JobState| !matches!(t, JobState::Failed(_));
                if plat.has_ios() {
                    checks.push(!s.apple_identity.trim().is_empty());
                    checks.push(self.profile.read().as_ref().is_some_and(|info| {
                        profile_checks(info, &p.ios_bundle_id, &s.apple_identity)
                            .iter()
                            .all(|c| c.ok)
                    }));
                    checks.push(creds.app_store_ok() && untested_or_ok(&self.asc_test.read()));
                }
                if plat.has_android() {
                    checks.push(creds.play_ok() && untested_or_ok(&self.play_test.read()));
                    checks.push(self.android.read().ok());
                }
                // Hygiene applies whatever the platform: no secrets in git,
                // and a backup that covers the current keys.
                checks.push(self.tracked_secrets.read().is_empty());
                checks.push(matches!(*self.backup.read(), BackupState::Current(_)));
                match checks.iter().filter(|ok| **ok).count() {
                    n if n == checks.len() => StepStatus::Done,
                    0 => StepStatus::Todo,
                    _ => StepStatus::Attention,
                }
            }
            Step::Version => {
                if valid_version(p.version.trim())
                    && p.ios_build_number > 0
                    && p.android_version_code > 0
                {
                    StepStatus::Done
                } else {
                    StepStatus::Attention
                }
            }
            Step::Build => {
                let a = self.artifacts.read();
                match &*self.build_phase.read() {
                    BuildPhase::Running(_) => return StepStatus::Running,
                    BuildPhase::Error(_) => return StepStatus::Attention,
                    _ => {}
                }
                let ios_ok = !plat.has_ios() || a.ipa.is_some();
                let android_ok = !plat.has_android() || a.aab.is_some();
                if ios_ok && android_ok {
                    StepStatus::Done
                } else if a.ipa.is_some() || a.aab.is_some() {
                    StepStatus::Attention
                } else {
                    StepStatus::Todo
                }
            }
            Step::Languages => {
                let filled = p
                    .locales
                    .iter()
                    .filter(|l| !p.sources_for(l).is_empty())
                    .count();
                if filled == 0 {
                    StepStatus::Todo
                } else if filled == p.locales.len() {
                    StepStatus::Done
                } else {
                    StepStatus::Attention
                }
            }
            Step::Screenshots => match &*self.gen_phase.read() {
                AppPhase::GeneratingAi | AppPhase::GeneratingManual | AppPhase::Resizing => {
                    StepStatus::Running
                }
                AppPhase::Error(_) => StepStatus::Attention,
                _ if p.output_paths.iter().any(|(_, path)| path.exists()) => StepStatus::Done,
                _ => StepStatus::Todo,
            },
            Step::Metadata => {
                let complete = |loc: &String| {
                    p.store_texts.get(loc).is_some_and(|t| {
                        !t.description.trim().is_empty()
                            && (!plat.has_android() || !t.short_description.trim().is_empty())
                    })
                };
                let started = |loc: &String| {
                    p.store_texts
                        .get(loc)
                        .is_some_and(|t| *t != StoreText::default())
                };
                if p.locales.iter().all(complete) {
                    StepStatus::Done
                } else if p.locales.iter().any(started) {
                    StepStatus::Attention
                } else {
                    StepStatus::Todo
                }
            }
            Step::Submit => {
                let mut kinds = Vec::new();
                if plat.has_ios() {
                    kinds.extend([
                        JobKind::AppStore,
                        JobKind::AppStoreBuild,
                        JobKind::AppStoreReview,
                    ]);
                }
                if plat.has_android() {
                    kinds.extend([JobKind::GooglePlay, JobKind::PlayRelease]);
                }
                let states: Vec<JobState> = kinds.into_iter().map(|k| self.job_state(k)).collect();
                // Shipped = the current version reached review (iOS) or a track (Play).
                let version = p.version.trim();
                let shipped = |platform: &str, action: &str| {
                    p.releases.iter().any(|r| {
                        r.platform == platform
                            && r.version == version
                            && r.action.starts_with(action)
                    })
                };
                if states.iter().any(|s| matches!(s, JobState::Running(_))) {
                    StepStatus::Running
                } else if states.iter().any(|s| matches!(s, JobState::Failed(_))) {
                    StepStatus::Attention
                } else if (!plat.has_ios() || shipped("ios", "Submitted for review"))
                    && (!plat.has_android() || shipped("android", "Released to"))
                {
                    StepStatus::Done
                } else {
                    StepStatus::Todo
                }
            }
        }
    }
}
