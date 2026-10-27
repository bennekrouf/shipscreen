# ShipScreen

![ShipScreen's Screenshots step: the iOS and Android sizes to generate, in a manual or AI style](docs/screenshot.png)

Make app-store screenshots, publish them, and build your app — from one
desktop app. Built with 🦀 Rust + [Dioxus](https://dioxuslabs.com) 0.7.

Runs on macOS, Windows and Linux, and on Android.

## Features

Each project opens in a workspace with one screen per step, in the order the
work happens: **App → Accounts → Version → Build → Languages → Screenshots → Store text →
Submit**.
The sidebar shows each step's status (to do, needs attention, running, done),
and a jobs drawer at the bottom keeps builds, generation and uploads running
while you move between steps.

### Projects

- Open an existing project folder, or create a new Dioxus project from the
  wizard (app name, slug, bundle ID, platform).
- Platforms: **iOS**, **Android**, **iOS + Android** or **Desktop**.
- Up to 8 recent projects are remembered.
- Per-project settings live in `<project>/shipscreen.json`: app name, slug,
  iOS bundle ID, Android package, logo, colours, languages and export targets.

### Screenshot generation

- **Per-language sources** — each language tab has its own screenshots and
  title/subtitle per image. Languages: Arabic, English, French, Hindi,
  Indonesian, Malay, Albanian, Turkish, Urdu, Chinese (Simplified).
- **AI mode** ([fal.ai](https://fal.ai)) — describe a background style; fal.ai
  generates a device frame with a flat grey screen, and ShipScreen finds that
  area and composites your screenshot into it. Every frame in a set shares the
  same theme. The last 20 style prompts are kept.
- **Manual mode** — title and subtitle drawn in Roboto Bold over your primary /
  secondary colours, with the text colour picked for contrast.
- **Export sizes**

  | Platform | Targets |
  | --- | --- |
  | iOS | iPhone 6.9″, 6.7″, 6.5″, iPad Pro 12.9″ |
  | Android | Phone (1080×2340), Feature graphic (1024×500) |
  | Desktop | 1280×800, 1280×720, 1920×1080 |

  Desktop projects use a landscape canvas (the screenshot is fitted whole under
  the captions, or placed in an AI-drawn laptop frame).

  Images are saved inside the project, in fastlane's layout: iOS to
  `fastlane/screenshots/ios/<locale>/`, desktop to
  `fastlane/screenshots/desktop/<locale>/`, Android to
  `fastlane/metadata/android/<play-language>/images/` (`phoneScreenshots/NN.png`,
  plus `featureGraphic.png` from the first screen).

### Store text

Per language: description, what's new, App Store keywords and promotional
text, Play short description, plus a support URL — with the stores' character
limits shown as you type, and "copy from English" to start a translation.

### Publishing

Direct API calls — fastlane is not required. The Submit step runs each store
as numbered stages, with a readiness checklist and a release history.

- **App Store** —
  1. *Listing*: store text and screenshots per language (languages missing in
     App Store Connect are added).
  2. *Build*: uploads the newest IPA with `xcrun altool` using the API key (macOS).
  3. *App Review*: waits for the build to process, answers export compliance
     if you declare exempt encryption, attaches it to the version and submits
     it for review.
- **Google Play** —
  1. *Listing*: store text, phone screenshots and feature graphic per language.
  2. *Release*: uploads the newest AAB, puts it on a track (internal, closed,
     open, production) as draft or rollout, with What's new as release notes.

Credentials are read from `fastlane/.env` or the environment
(`APP_STORE_CONNECT_API_KEY_*`, `GOOGLE_PLAY_JSON_KEY`, `ANDROID_PACKAGE_NAME`).

### Accounts & signing

- Signing identity from the keychain (shared), provisioning profile per project.
- The profile is checked against the app: bundle ID match (wildcards included),
  expiry, App Store distribution type, and same team as the signing identity.
- **Test connection** for App Store Connect (authenticates and finds the app by
  bundle ID) and Google Play (authenticates and opens/discards an edit).

### Version & build numbers

Each project owns its version (e.g. `1.2.0`), its next iOS build number
(CFBundleVersion) and its next Android versionCode, with Major/Minor/Patch
bumps and optional auto-increment after each successful release build. Older
projects are seeded from `build_number.txt` and `Dioxus.toml`, so numbers never
go backwards.

### Building

ShipScreen writes build scripts into the project folder
(`build_ios_distribution.sh`, `build_android_release.sh`, `build_android.sh`,
`build_apk.sh`), injecting the version and build numbers, and runs them from
the **Build** step with live output:

- **Build iOS IPA** — signed with your distribution identity and provisioning profile.
- **Build AAB** — for Google Play.
- **Build APK** — for a test device.

The project logo is written to the file the App step's **Written to** names
(`assets/icon.png` by default, where a Dioxus build looks for it) before each
build. A project that keeps its icon somewhere a PNG can't simply be dropped —
an asset catalogue, `res/mipmap-*` — leaves the field empty.

### Projects not built with Dioxus

The scripts above build Dioxus projects (`dx build` / `dx bundle`), and the
**New Project…** wizard only scaffolds those. Everything else in ShipScreen is
independent of how the app is written, so an Xcode/Swift or Gradle/Kotlin
project can use the same workflow — open its folder with **Open Existing…**:

- The App step fills itself in from the project's own files, whichever they
  are: an Xcode project's bundle identifier, display name, marketing version
  and build number, or a Gradle project's `applicationId`, `versionName`,
  `versionCode` and app label — Groovy or Kotlin, `app/build.gradle` either
  way. The Version step carries on from the number the project last used.
- The Build step's checks drop the ones that don't apply: no `dx`, no Rust
  targets, no `Dioxus.toml`. What's left is what any project needs — the
  Android SDK, NDK, build tools and Java, Xcode and the signing identity —
  and the Android API level is read from your own Gradle file.
- **Build it your way.** Either build in Xcode or with Gradle and copy the
  `.ipa` or `.aab` into the project folder, or put your own
  `build_ios_distribution.sh` / `build_android_release.sh` there and press
  Build. ShipScreen writes none of its own scripts into a project it can't
  build, so the names are yours to take; a script it didn't write is never
  replaced and runs as it is, with its output in the jobs drawer. On Windows,
  name it `build_android_release.cmd` (or `.bat`, or `.ps1`) and it runs
  without bash — those come first there, and a `.ps1` works on macOS and
  Linux too if PowerShell is installed.
- **Every script gets the project's settings as environment variables**, which
  is how your own script reaches the App and Version steps:

  | Variable | From |
  | --- | --- |
  | `APP_NAME`, `PROJECT_SLUG` | App step |
  | `IOS_BUNDLE_ID`, `ANDROID_PACKAGE_NAME` | App step |
  | `VERSION` | Version step |
  | `BUILD_NUMBER` | Version step (iOS CFBundleVersion) |
  | `VERSION_CODE` | Version step (Android versionCode) |
  | `SIGNING_IDENTITY`, `PROVISIONING_PROFILE` | Accounts step |
  | `ANDROID_KEYSTORE_PATH`, `ANDROID_KEY_ALIAS` | Android upload key |
  | `ANDROID_KEYSTORE_PASSWORD` | the system password store |
  | `JAVA_HOME`, `PATH` | the JDK picked in the Java card |
  | `SHIPSCREEN`, `CI` | both `1` |

  Anything set in the project's `.env` or `fastlane/.env` wins over these. With
  auto-increment on, the numbers are bumped only after a build ShipScreen's own
  scripts ran — a build of yours used whatever its script decided.
- **Built IPA** and **Built AAB** on the Build step say where your build
  leaves the file, when that isn't the project folder: a file or a folder,
  relative to the project or absolute (`~` works). Xcode's export folder and
  `app/build/outputs/bundle/release` are the usual answers; the APK next to
  the AAB is found the same way, and nothing needs copying by hand.
- Screenshots, store text and Submit work unchanged. The Build step and the
  uploads take the newest `.ipa` / `.aab` of those places, whatever
  produced it, and the version and build number that reach the stores are read
  from that binary. Screenshots are written in fastlane's layout, so a project
  that already uses fastlane finds them where it expects them.

The icon is copied to wherever **Written to** points before each build, and
the build says so in its log; a folder that isn't there means the step is
skipped, not that the build fails.

### App

- **Settings** (gear icon): fal.ai key, device style, inference steps. Stored
  in the user config directory (`shipscreen/settings.json`).
- Light / dark theme, following the system theme on desktop.
- Update check against `mayorana.ch`, with a banner when a new version is out.
- Dismissible notices published on `mayorana.ch`.
- On Android, images are picked through the Storage Access Framework and
  projects are kept in app-private storage.

## Use from Claude (MCP)

`shipscreen mcp` serves the projects to an MCP client over stdio, with no
window (`src/workspace/mcp.rs`); the **MCP** button in a project's sidebar
shows the Claude Desktop config and the Claude Code command. The tools:
`list_projects`, `get_project`, `set_app`, `set_version`, `set_languages`,
`set_screens`, `set_store_text`, `generate_screenshots` (the manual style) and
`build`. They run the workspace's own code: `jobs::generate_manual_images`,
`jobs::build` with `BuildInputs::problem` checked first, and the same seeding
of a project's identity and numbers as opening it. Store uploads are not
exposed.

The server and the window share `shipscreen.json`: the server reads, changes
and writes it in one go, and an open project reloads it when another process
wrote it (checked every 2 s).

## Development

```bash
# Run the desktop app
dx serve

# Android: build, check and install on a USB-connected device
./build_android.sh
./check_android.sh
./install_android.sh
```

Android requires Android Studio with the NDK and USB debugging enabled
(package `com.mayorana.appscreens`, kept from the app's name before ShipScreen; min SDK 24).

Releases are cut with `scripts/release.sh`; the GitHub workflow builds macOS,
Windows (MSI) and Linux (deb, AppImage) packages.

---

## Licence

Source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE).

- **Free** for personal use, learning, research and hobby projects, and for
  charities, schools, universities and government institutions.
- **Commercial use requires a licence** — including a solo consultant using it
  on client work, and an employee using it at their job.
  [Get in touch](https://mayorana.ch/en/contact).

This is deliberately not an OSI-approved open source licence: the source is
public and readable, but companies using it for work buy a licence.

The name, logo and icons are trademarks and are not covered by that licence —
fork it and rebrand it. See [TRADEMARK.md](TRADEMARK.md).
