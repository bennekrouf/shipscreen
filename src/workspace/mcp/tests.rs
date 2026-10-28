use super::*;
use std::collections::HashSet;

#[test]
fn every_tool_has_a_unique_name_and_an_object_schema() {
    let tools = tools();
    let tools = tools.as_array().unwrap();
    let names: HashSet<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), tools.len());
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        for required in t["inputSchema"]["required"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let key = required.as_str().unwrap();
            assert!(
                t["inputSchema"]["properties"].get(key).is_some(),
                "{} requires {key} undescribed",
                t["name"]
            );
        }
    }
}

#[test]
fn languages_are_the_ones_shipscreen_has() {
    assert_eq!(known_locale("fr-fr").unwrap(), "fr-FR");
    assert!(known_locale("de-DE").is_err());
}

#[test]
fn a_project_must_be_an_absolute_folder() {
    assert!(Project::from(&json!({ "project": "app" })).is_err());
    assert!(Project::from(&json!({ "project": "/nowhere/app" })).is_err());
}

#[test]
fn the_desktop_entry_starts_this_app_in_mcp_mode() {
    let config: Value = serde_json::from_str(&desktop_config(
        "/Applications/ShipScreen.app/Contents/MacOS/shipscreen",
    ))
    .unwrap();
    assert_eq!(
        config["mcpServers"]["shipscreen"]["command"],
        "/Applications/ShipScreen.app/Contents/MacOS/shipscreen"
    );
    assert_eq!(config["mcpServers"]["shipscreen"]["args"], json!(["mcp"]));
    let path = r"C:\Program Files\ShipScreen\shipscreen.exe";
    let config: Value = serde_json::from_str(&desktop_config(path)).unwrap();
    assert_eq!(config["mcpServers"]["shipscreen"]["command"], path);
    assert_eq!(
        code_command("/A B/shipscreen"),
        "claude mcp add shipscreen --scope user -- \"/A B/shipscreen\" mcp"
    );
}

#[test]
fn the_codex_command_quotes_a_path_with_spaces() {
    assert_eq!(
        codex_command("/A B/shipscreen"),
        "codex mcp add shipscreen -- \"/A B/shipscreen\" mcp"
    );
}
