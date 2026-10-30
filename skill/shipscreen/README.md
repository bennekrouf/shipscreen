# ShipScreen skill

A skill for AI agents that use **ShipScreen**, the desktop app that prepares a mobile app for the App Store and Google Play: identity, versions, store languages, captioned screenshots, store text and release builds. The
skill tells the agent how to use ShipScreen's tools well: which to call first, what
needs your yes, sensible defaults, and how to report back.

## What you need

- **ShipScreen**, installed from <https://mayorana.ch/en/apps/shipscreen> (macOS,
  Windows or Linux). This skill does not include the app.
- A desktop agent connected to ShipScreen's MCP server: Claude Desktop, Claude
  Code, Codex CLI or Cursor. `references/connect.md` has the steps. Web-only
  agents such as ChatGPT can't reach an app on your computer.

## What you can ask for

- List what still blocks a build or a submission
- Write and translate store text within each store's limits
- Add store languages and caption screenshots
- Generate store-ready screenshots
- Bump the version and run an IPA, AAB or APK build

## What waits for your yes

- Saving store text over yours (shown with each field's length first)
- Starting a build, which takes minutes

## Limits

Uploading to the stores is done by you, from the Submit step in ShipScreen's window.

## Install

- **Claude Code**: unzip into `~/.claude/skills/` so the folder is
  `~/.claude/skills/shipscreen/`.
- **claude.ai or Claude Desktop**: upload the zip under Skills in Settings.
- **Other agents**: put the `shipscreen` folder where the agent reads skills.

## Help

Questions, licences and bug reports: <https://mayorana.ch/en/contact>.
