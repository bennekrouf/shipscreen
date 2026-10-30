---
name: shipscreen
description: How to get a mobile app store-ready with ShipScreen, the desktop app that holds an app's identity, version and build numbers, store languages, captioned screenshots, App Store and Google Play text, and release builds. Read this skill before calling any shipscreen tool (list_projects, get_project, set_app, set_version, set_languages, set_screens, set_store_text, generate_screenshots, build), because it has the store-text writing rules, character-limit handling, what needs the user's yes, and how to report what is missing. Use it whenever the user mentions ShipScreen, or asks to write, shorten or translate an App Store or Play Store description, keywords, promotional text, short description or what's new, add store languages, caption or generate store screenshots, bump a version or build number, check what blocks a release, or build an IPA, AAB or APK.
---

# ShipScreen

ShipScreen is a desktop app (macOS, Windows, Linux) that takes a mobile app
from a project folder to store-ready: the app's identity, version and build
numbers, store languages, raw screenshots with captions per language, store
text within each store's limits, the store-ready screenshot images, and the
release builds. It is built for Dioxus projects and works with others too.
Projects are named by their folder's absolute path.

The person you are helping is shipping their own app and wants the store
listing done well in every language, without fighting character limits.

## Before you start

The tools come from ShipScreen's own MCP server (`shipscreen mcp`, over
stdio). If no tool named `list_projects` from a `shipscreen` server is
available, the app is not connected:

- Tell the user ShipScreen is a separate desktop app, from
  <https://mayorana.ch/en/apps/shipscreen>, and that this skill only works
  with it installed. Do not download or install it for them.
- Once installed, the **MCP** button in a project's sidebar shows the exact
  config for Claude Desktop and the command for Claude Code and Codex.
  `references/connect.md` has the steps per client.

## How to work with it

1. **`list_projects`** gives the projects opened in ShipScreen, most recent
   first. A project must have been opened in the window once.
2. **`get_project`** shows everything about one, including what is still
   missing for a build. Start every task here and work from that list.
3. **Identity and version.** `set_app` changes only the fields given (app
   name, support URL, screenshot colours, encryption exemption, Play track
   and release status). `set_version` sets or bumps the version the stores
   show (`major`, `minor`, `patch`) and the build numbers.
4. **Languages.** `set_languages` adds or removes store languages; English
   (en-US) is always kept.
5. **Store text.** `set_store_text` sets one language's text: description,
   what's new, App Store keywords and promotional text, Play short
   description. Only the fields given change, and a field over its store
   limit is refused, so count before saving.
6. **Screenshots.** `set_screens` sets one language's raw screenshots (PNG or
   JPEG paths) and their captions (a title and subtitle drawn above each).
   `generate_screenshots` draws the store-ready images for every language in
   the simple style; the AI style is only in the ShipScreen window.
7. **`build`** runs a release build: `ios` (a signed IPA, on a Mac with the
   signing set up), `aab` or `apk`. It takes minutes; tell the user before
   starting, and report the result or the first error from its log.

## Writing store text

This is where you help most. Write for people browsing a store, not for
developers:

- **Description**: lead with what the app does for the user in the first
  two lines (that's what shows before "more"), then the main features as
  short lines, then anything that builds trust. No claims you can't back.
- **App Store keywords**: comma-separated, no spaces after commas, no words
  already in the app name, no competitors' names or trademarks.
- **Promotional text** and **Play short description**: one sentence on the
  main benefit.
- **What's new**: what the user will notice, in plain words.
- **Translations**: translate meaning, not word for word, and pick keywords
  people search for in that language.

Always show the user the text before saving it over theirs, with each field's
length against its limit, and wait for their yes.

## Not available here

Uploading to App Store Connect or Google Play is not exposed. When the
listing and build are ready, tell the user to finish from the **Submit** step
in the ShipScreen window, which runs each store's stages with a checklist.

## How to report back

For what is missing before a build or a submission, use a checklist, most
blocking first, each with the step in ShipScreen where it is fixed:

- [ ] Support URL — *App* step
- [ ] Store text in de-DE — *Store text* step

For store text, show each field with its length against its limit before
saving it:

| Language | Field | Length / limit | Text |
|---|---|---|---|

After `build`, say whether it succeeded, the version and build number, and the
file's path; if it failed, the first error line from the log and what it
usually means.
