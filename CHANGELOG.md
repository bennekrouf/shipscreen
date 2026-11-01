# Changelog

What changed in each release of **ShipScreen**, the desktop app for making
app-store screenshots, publishing them and building your app.

The public version of this page — with the download for each release — lives at
<https://mayorana.ch/en/apps/shipscreen/releases>. It is generated from this
file by `scripts/changelog_to_json.py`, so this file is the only place a
release note is written.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each heading is dated on the day its tag was pushed. Releases that carried only
build or packaging work say so rather than being hidden: the version numbers a
user sees in the update prompt should all be accounted for.

## [0.1.58] - 2026-11-01

### Changed

- Build and packaging only — no user-visible change.

## [0.1.57] - 2026-11-01

### Added

- Install & restart, on the banner that says a new version is out: one click
  downloads it, checks it, puts it in place of the version you have and opens it
  again, on macOS, Windows and Linux. No disk image to open, drag and throw
  away, no setup to click through. If an update can't finish, ShipScreen says
  why the next time it opens. Where it can't replace itself (an app in a folder
  you can't change), the banner offers the download instead.

## [0.1.56] - 2026-11-01

### Changed

- Build and packaging only — no user-visible change.

## [0.1.55] - 2026-11-01

### Changed

- Using ShipScreen from Claude, Codex or another agent takes less of the agent's
  context on every message, including ones that have nothing to do with
  ShipScreen: its instructions are a few lines, and what a setting every tool
  takes means is said once rather than in each tool.

## [0.1.54] - 2026-10-31

### Changed

- When you use ShipScreen from Claude, Codex or another agent, the agent now
  reads the ShipScreen skill first if you have it installed, so it follows the
  skill's rules on what needs your yes and lays out its answers the way the
  skill describes.

## [0.1.53] - 2026-10-31

### Changed

- ShipScreen's answers to Claude and other agents are shorter, so a long
  session spends fewer tokens: empty fields are left out and nothing is
  padded with spacing.
- Reading a project shows how full each store text field is in every
  language, and the text itself only for the language asked for. Asking
  about a version or a build no longer brings back every description in
  every language.

## [0.1.52] - 2026-10-31

### Changed

- Packaging only — no user-visible change.

## [0.1.51] - 2026-10-30

### Changed

- Packaging only — no user-visible change.

## [0.1.50] - 2026-10-30

### Changed

- Packaging only — no user-visible change.

## [0.1.49] - 2026-10-28

### Changed

- The update banner and the occasional message from mayorana.ch now sit just
  under the bar at the top of the window instead of covering it, and look the
  same in every mayorana app. Download opens the new build in your browser and
  shows its SHA-256 when one is published, to compare with the file you
  downloaded.
- The Pro window looks and reads the same in every mayorana app. Once you're Pro
  it names your plan and email, and pressing Enter in the licence number field
  activates it.

## [0.1.48] - 2026-10-28

### Changed

- Packaging only — no user-visible change.

## [0.1.47] - 2026-10-28

### Changed

- The bar across the top of the window now looks and works the same in every
  mayorana app: the name and version on the left, and on the right Settings,
  MCP, Feedback, the light/dark switch and the Pro badge in the corner.
- Settings, MCP and Feedback, at the bottom of the project's step list until
  now, and Pro, on the first screen only, are in the bar on every screen.
- The update banner sits under the bar instead of covering the top of the
  window.

## [0.1.46] - 2026-10-28

### Changed

- Packaging only — no user-visible change.

## [0.1.45] - 2026-10-27

### Added

- You can now use ShipScreen from Codex: open the MCP settings, copy the new
  command, and paste it into a terminal to add ShipScreen as a server.

## [0.1.44] - 2026-10-27

### Changed

- Packaging only — no user-visible change.

## [0.1.43] - 2026-10-26

### Fixed

- On Windows, the ShipScreen logo on the first screen shows again instead
  of a broken image.

## [0.1.42] - 2026-10-25

### Fixed

- Installing silently, as IT does for an unattended update with `/VERYSILENT`,
  no longer hangs when the same version, or an older ShipScreen installed from
  an .msi, is already there: setup waited on a question nobody could see. It
  now goes ahead without asking.

## [0.1.41] - 2026-10-25

### Changed

- Activating a licence now shows a small spinner while it is checked, then
  a window that says you're Pro, with the AI credits left and the renewal
  date in small type below. Removing the licence and checking it again no
  longer sit beside it; Manage subscription is a small link. Press Esc or
  the × to close it.

## [0.1.40] - 2026-10-23

### Changed

- The new-version banner and the usage-statistics notice now look the same
  as in GitAgent: a blue bar across the top, or the bottom for the notice,
  with a Download link and a × to hide it.
- Every action button is now 30px tall, borders included, like the
  regular buttons beside it.

## [0.1.39] - 2026-10-23

### Changed

- Dialogs now have a title bar with a × to close them, and text fields have
  the same size and typeface as in GitAgent.
- An open dialog now dims the light/dark switch in the corner along with the
  rest of the window, instead of leaving it lit on top.
- The buttons along the top of the window now all have the same height,
  borders included, and the light/dark switch matches them.

## [0.1.38] - 2026-10-23

### Changed

- Buttons now have the same size, rounded corners and typeface as in
  GitAgent, so they look alike across the Mayorana apps.

## [0.1.37] - 2026-10-23

### Added

- **Written to** under App icon: the file the icon is copied to before each
  build. It stays `assets/icon.png`, where a Dioxus build looks for it, and a
  project that keeps its icon in an asset catalogue or `res/mipmap-*` can
  empty the field instead of having a stray PNG written into it.

### Changed

- A build script you wrote can be a `.cmd`, a `.bat` or a `.ps1` instead of a
  bash script. On Windows those are preferred over a `.sh`, so a build no
  longer needs Git for Windows installed; a `.ps1` runs on macOS and Linux too
  where PowerShell is. The jobs drawer names the file that actually ran.

## [0.1.36] - 2026-10-23

### Added

- **Built IPA** and **Built AAB** on the Build step: where your own build
  leaves the file, when that isn't the project folder. Point them at Xcode's
  export folder or `app/build/outputs/bundle/release` — a file or a folder,
  relative to the project or absolute — and the Build and Submit steps find
  the newest build there, with nothing to copy by hand. Projects whose builds
  land in the project folder, as ShipScreen's own scripts do, need neither.
- The App step now fills itself in from an Xcode or Gradle project too: bundle
  identifier, package, display name and version, and the build number the
  project last used, so a project that isn't built with Dioxus no longer
  starts with empty fields.

### Changed

- A project that isn't built with Dioxus keeps its folder to itself: the Build
  step no longer writes its own build scripts next to yours, so the names are
  free for a script of your own. Put `build_ios_distribution.sh` or
  `build_android_release.sh` in the project folder and the Build step runs it
  as it is — an Xcode or Gradle build works from there, and the screenshots,
  store text and Submit steps carry on as usual.
- A build script you wrote now receives everything the App, Version and
  Accounts steps know, as environment variables: app name, slug, version, iOS
  build number, Android versionCode, both identifiers, the signing identity and
  the provisioning profile. It no longer has to keep its own copy of the
  version. Anything set in the project's `.env` or `fastlane/.env` still wins,
  and the README lists the names.

### Fixed

- With auto-increment on, the build number no longer climbs after a build run
  by a script of your own: that build used the number in the script, not the
  one in the Version step, so counting it would have skipped a number at the
  store.
- Building Android in a project ShipScreen can't build itself now says what to
  do instead of failing somewhere inside a Dioxus build.
- The Build step no longer reports missing Dioxus tools as problems in a
  project that doesn't use them: a project built by Xcode or Gradle is checked
  for the Android SDK, Java, Xcode and the signing identity, and its Android
  API level is read from its own Gradle file.

## [0.1.35] - 2026-10-22

### Changed

- Packaging only — no user-visible change.

## [0.1.34] - 2026-10-22

### Changed

- The window title now shows the app version and a Pro or Pro inactive badge,
  and updates as soon as your licence status changes.

## [0.1.33] - 2026-10-22

### Changed

- One licence number for every Mayorana app on this computer. Paste it in any
  of them and the others are Pro too the next time they start, with nothing to
  paste again; a number already pasted in one app is shared the same way.
  Removing the licence in the Pro window removes it from all of them.

## [0.1.32] - 2026-10-21

### Added

- Free AI credits every month, without Pro. Enter your email under Get free
  AI credits in the Pro window and the licence number arrives by email; paste
  it in the same window. The window then shows how many credits are left this
  month. The free version's limits stay as they are.

## [0.1.31] - 2026-10-21

### Changed

- ShipScreen Pro now comes with a monthly plan on mayorana.ch instead of a
  one-time licence key. Paste the licence number from the plan's email (like
  ABCDE-FGHJK-LMNPQ-RSTUV) in the Pro window. ShipScreen checks it when it
  starts and once a day, and Pro keeps working for a week without a
  connection.
- The Pro window shows your plan, the AI credits left this month and when the
  plan renews. Manage subscription opens the page where you change or cancel
  it.

## [0.1.30] - 2026-10-20

### Added

- A Feedback button under Settings and MCP, and Feedback and mayorana.ch links
  on the projects screen, open the contact form with ShipScreen and your
  version already filled in, so an idea or a problem reaches us in two clicks.

## [0.1.29] - 2026-10-19

### Changed

- ShipScreen now links the Windows C runtime statically, ensuring it runs on any
  PC without requiring the Visual C++ Redistributable. This makes the
  application more portable and easier to distribute.

## [0.1.28] - 2026-10-18

### Added

- Use ShipScreen from Claude. The new **MCP** button at the bottom of a
  project's sidebar shows what to paste into Claude Desktop, or the one line
  to run for Claude Code, already pointing at where ShipScreen is installed.
  Claude can then read everything about a project and what it still needs,
  write and translate the store text (the stores' character limits are
  enforced), add languages, add screenshots and caption them in every
  language, set or bump the version and build numbers, generate the
  screenshots and run the iOS, AAB and APK builds. What Claude changes shows
  up in the open project within a couple of seconds. Uploading to the stores
  stays in the Submit step.

## [0.1.27] - 2026-10-16

### Added

- ShipScreen Pro. The free version works on one app, with every step from
  build to store submission; Pro works on any number of apps, and is the
  licence for commercial use. The first app you open becomes your free app.
  Opening another one asks for Pro, or offers to make that one your free app
  instead. Buy it on [ShipScreen's page](https://mayorana.ch/en/apps/shipscreen),
  then click **Get Pro…** on the start screen and paste the key. The key is
  checked on your computer: no account, nothing sent anywhere.

## [0.1.26] - 2026-10-14

### Changed

- AppScreens is now called ShipScreen. Your projects, saved passwords, signing
  keys and Java setup carry over. A project's `appscreens.json` is still read
  and kept where it is; new projects get a `shipscreen.json`. Build scripts
  AppScreens wrote are still recognised as ShipScreen's own and kept up to
  date. On macOS the update installs as ShipScreen.app; once it runs, you can
  delete AppScreens.app from Applications.

## [0.1.25] - 2026-10-07

### Added

- Android builds now work on Windows and Linux, not just macOS. ShipScreen runs
  the whole build itself — `dx`, then the icon, package name, version and
  signing fixes, then Gradle — instead of a bash script, finds the Android SDK
  where Android Studio puts it on each system, and on Windows keeps the
  upload-key password in Windows Credential Manager.
- The keystore password no longer gets written into the generated Gradle
  project: Gradle reads it from the environment for the length of the build.

### Changed

- On Windows, ShipScreen now installs from `shipscreen-windows-setup.exe`
  instead of an `.msi`. It installs for your user without administrator
  rights, and removes an ShipScreen installed from the old `.msi` first. Your
  projects and settings are kept.
- A build script you wrote yourself still runs as it is. On Windows that needs
  Git for Windows' bash, and ShipScreen says so if it's missing.
- On Windows and Linux, the iOS build button and the Apple signing tools say
  they need a Mac, instead of failing halfway. Screenshots, store text and
  listing uploads for iOS work everywhere.
- The Doctor's "Open Android Studio" button works on Windows and Linux too.

### Fixed

- Windows no longer opens a console window behind ShipScreen, or one for
  every tool it runs.

## [0.1.24] - 2026-10-07

### Added

- The Windows installer now includes a license page that users must accept
  before installing the application. This ensures that users are aware of the
  licensing terms before proceeding with the installation.

## [0.1.23] - 2026-10-06

### Added

- ShipScreen now shares anonymous usage statistics: whether it is installed and
  opened, its version and your operating system. It is on by default; a note at
  the bottom of the window tells you once, and nothing is sent before you have
  seen it. **Turn off** there stops it for good and deletes anything not yet
  sent. Your files, data and accounts are never part of it. It also stays off if
  `DO_NOT_TRACK`, `DISABLE_UPDATE_CHECK` or `MAYORANA_NO_TELEMETRY` is set.

## [0.1.22] - 2026-10-01

### Added

- The Android upload key card lists the keystores already on your Mac (in your
  keys folder and the project) with a Use button, so picking yours no longer
  means browsing for it.

### Changed

- Building an AAB without an upload key no longer starts a build that fails
  halfway: the Build step shows the missing key with a link to the Accounts
  step, and the build button says where to set it up. The build script's own
  message now points to the same place before mentioning the project's `.env`.

### Fixed

- Opened from the Dock or Finder, ShipScreen no longer reports "dx not found"
  or missing Rust targets when the Dioxus CLI and rustup are installed. It now
  finds the tools in your Rust install folder and your terminal's settings,
  and the build scripts it runs find them too.
- When another program called `dx` comes first (Homebrew's deno installs
  one), the check now says so instead of reporting a wrong Dioxus version, and
  builds use the Dioxus CLI.

## [0.1.21] - 2026-10-01

### Added

- The Google Play service-account key gets the same help. If another of your
  projects already uses one, one click reuses it. Otherwise the Google Play
  card walks you through it: enable the API, create the service account and
  its JSON key, and invite it in Play Console, each step linking to the right
  page. It then picks up the downloaded JSON key and keeps a private copy in
  your keys folder. The email to invite comes with a Copy button. Finally it
  saves the key's location to the project's `.env` and tests the connection.

### Changed

- The "Create a Distribution certificate" form explains each field, and warns
  when the keychain already has one: Apple allows very few per team, and your
  profiles are tied to the one you have.

## [0.1.20] - 2026-09-30

### Added

- Setting up the App Store Connect API key no longer means editing `.env` by
  hand. If another of your projects already has a key, one click reuses it —
  one key works for every app of your team. Otherwise the Apple card walks you
  through creating one, with a link straight to the right App Store Connect
  page. It then picks up the downloaded `.p8`, reads the Key ID from its file
  name, keeps a private copy in your keys folder (Apple lets you download it
  only once), saves everything to the project's `.env` and tests the
  connection.
- "Add profile…" next to the provisioning profile list installs a profile you
  downloaded from Apple Developer and selects it, with a link to your
  profiles there.

### Fixed

- Provisioning profiles downloaded by Xcode 16 now appear in the profile list;
  only the older profiles folder used to be read.

## [0.1.19] - 2026-09-29

### Added

- Opening a project you've already built fills in the App step for you: the
  app name, project slug, bundle ID and Android package come from your build
  scripts, `Dioxus.toml`, `Cargo.toml`, fastlane and your last builds, and a
  project that was only ever built for iOS (or Android) is set up for just
  that platform. Fields you've filled in yourself are never changed.

### Fixed

- ShipScreen no longer replaces a build script you wrote yourself. It used to
  rewrite every build script from its template before each build, losing
  changes like a display name or a minimum iOS version. Scripts it wrote are
  still kept up to date.

## [0.1.18] - 2026-09-29

### Added

- The Accounts step can now set up signing without a terminal: create an
  Android upload keystore (or add a key for another app to it) and get the
  certificate Play Console asks for, create an Apple Distribution certificate
  straight into your keychain, and regenerate an invalid App Store profile.
- Passwords for the upload keystore are kept in the macOS Keychain instead of
  a plain-text `.env` file (a `.env` still works and takes priority).
- The Accounts step warns when `.env` files, keys, keystores or service-account
  files are committed in the project, and can stop tracking them for you.
- An encrypted backup of your whole keys folder in one file, with a reminder
  until you have one and again whenever your keys change — so a lost or stolen
  Mac no longer means revoking and recreating every key.
- The Build step checks your build tools before you build: `dx` against the
  project's Dioxus version, the Android SDK, NDK, target platform, build tools
  and Java, the Rust targets, Xcode and your signing identity. Each problem says
  how to fix it, and several have a button that does it for you — instead of
  finding them one failed build at a time.
- Every build is inspected before you upload it: which key signed the AAB
  (compared with your upload key), its package, version and target API; and
  for the IPA its bundle ID, version, distribution signature and profile.
  Problems show under the build and in the Submit checklist.
- Releasing to Google Play stops before the upload when the bundle's
  versionCode isn't higher than one already on Play, instead of after it.
- The App step checks that your app's own code and `.env` use the same Android
  package as the app — store links, data paths and package constants — and
  can correct them in one click. A mismatch used to ship silently: saved data
  in the wrong place, a "rate us" link to a missing page, or an upload to a
  different app.
- The build check flags `Dioxus.toml` settings the installed `dx` ignores
  (`target_sdk_version`, `min_sdk_version`) or rejects (`permissions = [...]`),
  and a target API below what Google Play requires, and can rewrite them.
- The App step warns when the project's code isn't safe from a lost Mac: not
  in git, no remote, or commits that were never pushed.
- The Submit step shows what the stores have right now — version states from
  App Store Connect and every Play track's releases — with links to what only
  the consoles show.
- The Accounts step has an "If this Mac is lost or stolen" checklist: what to
  revoke and replace, in order, filled in with this project's own key IDs and
  linking straight to each console page.

- The Build step shows every Java installed on the machine — Android Studio's,
  Homebrew, SDKMAN, system installs, and on Windows the registry — marks the
  ones too old or too new for the Gradle version your project builds with,
  and lets you pick which one ShipScreen uses. Your terminal and system
  settings are left alone, so there is no JAVA_HOME to set up by hand.
- When no suitable Java is installed, the Java card offers to install one that
  fits your project: Homebrew (inside ShipScreen, no password) or the Temurin
  installer on macOS, your package manager in a terminal on Linux, winget on
  Windows — and picks it up as soon as it's done.
- Android builds no longer start on a Java the project's Gradle can't run on.
  If the Java you picked, or the one in `JAVA_HOME`, is too old or too new
  (Java 26 with Gradle 9.1, for example), the build uses one that fits and says
  so in the log. The Doctor and the Java card point it out, with a
  one-click "Use Java 21".
- When a build fails on a Java mismatch anyway, the Build step explains it in
  plain words and offers "Build again with Java 21", using a Java you already
  have installed.
- If you want the same Java in your own terminal, the Java card can add it to
  your shell profile (zsh, bash or fish) or your Windows user settings. You see
  the exact lines first, your profile is backed up, and Remove takes them out
  again. "Check my terminal" opens a fresh shell and shows which Java it and
  your project's Gradle really use.

### Fixed

- Signing and Android builds no longer fail with "Unable to locate a Java
  Runtime" when ShipScreen is started from the Dock or a terminal without Java
  on its path. It now finds a Java installation by itself — `JAVA_HOME`,
  macOS's registered Java, Android Studio's bundled runtime or Homebrew — and
  hands it to the build.

## [0.1.17] - 2026-09-28

### Changed

- Android release builds take their signing key from the project's `.env`
  (`ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`, and optionally
  `ANDROID_KEY_ALIAS`) instead of a fixed keystore location, so you can use
  your own upload key and keep its password out of the build script. The
  Accounts step shows whether the keystore is found and the password is set.

### Fixed

- Android release builds stop straight away with a clear message when the
  Android NDK is not installed, when the installed `dx` and the project's
  Dioxus version do not match (with the command that fixes it), or when `dx`
  fails before creating the Android project. They used to carry on and end in
  a cascade of unrelated "No such file" errors.

## [0.1.16] - 2026-09-28

### Changed

- Packaging only — no user-visible change.

## [0.1.15] - 2026-09-27

### Changed

- The update banner now links directly to the OS-specific build, making it
  easier to download the latest version.

## [0.1.14] - 2026-09-25

### Changed

- Packaging only — no user-visible change.

## [0.1.13] - 2026-09-24

### Fixed

- On wide windows the step content now fills the space beside the sidebar
  instead of sitting in a narrow centred column with an empty band next to it.

## [0.1.12] - 2026-09-24

### Changed

- Each project now opens in a workspace with one screen per step, in the order
  the work happens: App, Accounts, Version, Build, Languages, Screenshots,
  Store text and Submit. The sidebar shows each step's status (to do, needs
  attention, running, done).
- Builds, screenshot generation and uploads run in a jobs drawer at the bottom
  and keep going while you move between steps.
- The signing identity and provisioning profile moved from Settings to the
  Accounts step, and the app version to the Version step. The provisioning
  profile is now chosen per project, since each profile belongs to one app;
  Settings keeps the fal.ai key, device style and inference steps.

### Fixed

- Android screenshots are saved inside the project, one set per language, in
  fastlane's folder layout. They used to land wherever ShipScreen was started
  from, and each language overwrote the one before, so every language got the
  same images on Google Play.
- Google Play gets a single feature graphic per language, taken from the first
  screen. Every screen used to produce one, but Play keeps only one.
- Google Play uploads for Arabic, Hindi, Turkish and Chinese (Simplified) now
  use the language codes Play accepts; they were rejected before.
- Desktop screenshot sizes are exported. Desktop projects get a landscape
  layout: captions on top with the whole window fitted below, or an AI-drawn
  laptop frame.
- Unticking iOS or Android under Sizes stops that platform from being exported.

### Added

- A Store text step: description, what's new, App Store keywords and
  promotional text, Play short description and support URL per language, with
  each store's character limits shown as you type and "copy from English" to
  start a translation.
- The Submit step runs each store as numbered stages, with a readiness
  checklist and a release history. For the App Store it can wait for the build
  to process, answer export compliance and submit the version for review; for
  Google Play it uploads the newest AAB to the track you choose, as a draft or
  a rollout.
- The provisioning profile is checked against the app: bundle ID, expiry,
  distribution type and team. **Test connection** checks App Store Connect and
  Google Play credentials before you rely on them.
- Each project keeps its own version, next iOS build number and next Android
  versionCode.

## [0.1.11] - 2026-09-14

### Added

- A banner at startup for occasional messages from us, such as a request for
  feedback. It is fetched once from mayorana.ch, stays until you dismiss it and
  is not shown again after that. If the notice cannot be fetched, no banner
  appears and startup is not slowed.

## [0.1.10] - 2026-09-06

### Changed

- Packaging only — no user-visible change.

## [0.1.9] - 2026-09-06

### Changed

- A redesigned interface: a consistent type scale, spacing and colours in both
  themes, and line icons in place of the emoji that used to label buttons and
  tabs.
- Screenshot previews show the whole phone screen instead of cropping its top
  and bottom.
- Placeholder text is readable, and every control shows keyboard focus.

### Fixed

- On Android, layouts meant for small screens now apply. They were being
  ignored, so the app was laid out as if on a desktop-width page.

## [0.1.8] - 2026-08-30

### Changed

- Packaging only — the release pipeline can now sign the Windows installer.

## [0.1.7] - 2026-08-28

### Changed

- Packaging only — no user-visible change.

## [0.1.6] - 2026-08-27

### Changed

- Packaging only — no user-visible change.

## [0.1.5] - 2026-08-27

### Changed

- Downloads and update checks now come from mayorana.ch instead of GitHub. The
  update banner links to the download page there.
- ShipScreen is source-available under the PolyForm Noncommercial licence:
  free for personal, educational and noncommercial use.

## [0.1.4] - 2026-08-27

### Changed

- Packaging only — no user-visible change.

## [0.1.3] - 2026-08-27

### Added

- ShipScreen runs on Android, picking images through the system file picker.
- A banner when a newer version is available.
- A light and dark theme toggle, following the system theme until you choose.
- A new app icon.

### Fixed

- The packaged app no longer loses its styling. The stylesheet is now built
  into the app instead of being loaded from beside it.

## [0.1.2] - 2026-06-06

### Added

- A signed and notarized macOS build that opens with a normal double-click.

## [0.1.1] - 2026-06-06

### Added

- First release. Create a new Dioxus project from a wizard or open an existing
  one, for iOS, Android or desktop.
- Screenshot generation per language, with a title and subtitle on each image:
  either drawn over your own colours, or placed in a device frame generated by
  fal.ai from a style you describe.
- Export at the sizes each store requires for iPhone, iPad, Android phone, the
  Play feature graphic and desktop.
- Upload screenshots to App Store Connect and Google Play, and build signed iOS
  and Android releases, from the app.
