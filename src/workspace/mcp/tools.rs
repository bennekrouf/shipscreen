//! The tools ShipScreen offers an MCP client, as the client sees them.

use super::*;

pub(super) fn tools() -> Value {
    let project =
        json!({ "type": "string", "description": "Absolute path of the project folder." });
    let codes: Vec<&str> = LOCALES.iter().map(|(code, _)| *code).collect();
    let locale = json!({ "type": "string", "enum": codes, "description": "Store language code." });
    let mut store_fields = serde_json::Map::new();
    store_fields.insert("project".into(), project.clone());
    store_fields.insert("locale".into(), locale.clone());
    for (field, limit) in STORE_FIELDS {
        store_fields.insert(
            field.into(),
            json!({ "type": "string", "description": format!("At most {limit} characters.") }),
        );
    }
    json!([
        {
            "name": "list_projects",
            "description": "The projects opened in ShipScreen, most recent first, with their app name, platform and version.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "get_project",
            "description": "Everything about one project: identity, platform, version and build numbers, languages, \
                            screens and captions per language, store text with its limits, the screenshots and \
                            builds made so far, store credentials, what each build still needs, and the release history.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
        },
        {
            "name": "set_app",
            "description": "Changes the app's identity and listing settings. Only the fields given change.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "app_name": { "type": "string", "description": "Shown under the icon, and the Play title (at most 30 characters there)." },
                    "project_slug": { "type": "string", "description": "The Dioxus project's lowercase name." },
                    "ios_bundle_id": { "type": "string" },
                    "android_package": { "type": "string" },
                    "support_url": { "type": "string", "description": "Required before App Review." },
                    "primary_color": { "type": "string", "description": "#RRGGBB, behind odd screenshots." },
                    "secondary_color": { "type": "string", "description": "#RRGGBB, behind even screenshots." },
                    "exempt_encryption": { "type": "boolean", "description": "The app only uses exempt encryption (HTTPS and the like)." },
                    "play_track": { "type": "string", "enum": ["internal", "alpha", "beta", "production"] },
                    "play_release_status": { "type": "string", "enum": ["draft", "completed"] },
                },
                "required": ["project"],
            },
        },
        {
            "name": "set_version",
            "description": "Sets the version the stores show, or bumps it, and the build numbers the next builds use.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "version": { "type": "string", "description": "e.g. 1.2.0" },
                    "bump": { "type": "string", "enum": ["major", "minor", "patch"] },
                    "ios_build_number": { "type": "integer", "minimum": 1 },
                    "android_version_code": { "type": "integer", "minimum": 1 },
                    "auto_increment_build": { "type": "boolean", "description": "Raise the build number after each successful release build." },
                },
                "required": ["project"],
            },
        },
        {
            "name": "set_languages",
            "description": "Adds or removes store languages. English (en-US) is always kept.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "add": { "type": "array", "items": locale },
                    "remove": { "type": "array", "items": locale },
                },
                "required": ["project"],
            },
        },
        {
            "name": "set_screens",
            "description": "Changes one language's raw screenshots and their captions (title and subtitle drawn \
                            above each). Screens are numbered from 1 as get_project lists them before this call; \
                            captions are applied first, then removals, then additions at the end.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "locale": locale,
                    "captions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "screen": { "type": "integer", "minimum": 1 },
                                "title": { "type": "string" },
                                "subtitle": { "type": "string" },
                            },
                            "required": ["screen"],
                        },
                    },
                    "remove": { "type": "array", "items": { "type": "integer", "minimum": 1 } },
                    "add": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Absolute path of a PNG or JPEG." },
                                "title": { "type": "string" },
                                "subtitle": { "type": "string" },
                            },
                            "required": ["path"],
                        },
                    },
                },
                "required": ["project", "locale"],
            },
        },
        {
            "name": "set_store_text",
            "description": "Sets one language's store text. Only the fields given change; one over its limit is \
                            refused. description and whats_new go to both stores (Play keeps the first 500 \
                            characters of whats_new), keywords and promo_text to the App Store, short_description \
                            to Google Play.",
            "inputSchema": { "type": "object", "properties": store_fields, "required": ["project", "locale"] },
        },
        {
            "name": "generate_screenshots",
            "description": "Draws the store-ready screenshots for every language: each screen on the primary or \
                            secondary colour under its caption, at every export size, into the project's fastlane \
                            folders. Replaces the last ones.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
        },
        {
            "name": "build",
            "description": "Runs a build, as the Build step does: ios (a signed IPA for the App Store, on a Mac with \
                            Xcode), aab (a signed bundle for Google Play) or apk (for a test device). Takes several \
                            minutes. After a release build the build number goes up when auto-increment is on.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "target": { "type": "string", "enum": ["ios", "aab", "apk"] },
                },
                "required": ["project", "target"],
            },
        },
    ])
}
