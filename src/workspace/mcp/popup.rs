//! The window that shows how to use ShipScreen from Claude.

use super::*;

#[component]
pub(crate) fn McpPopup(on_close: EventHandler<()>) -> Element {
    let command = command();
    let desktop = desktop_config(&command);
    let code = code_command(&command);
    let codex = codex_command(&command);
    rsx! {
        div { class: "settings-backdrop", onclick: move |_| on_close.call(()) }
        div {
            class: "settings-popup settings-popup-wide",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Use ShipScreen from Claude or Codex",
            tabindex: "-1",
            onmounted: move |e: Event<MountedData>| {
                spawn(async move { let _ = e.set_focus(true).await; });
            },
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    e.stop_propagation();
                    on_close.call(());
                }
            },
            div { class: "modal-head",
                h2 { "Use ShipScreen from Claude or Codex" }
                button { class: "modal-close", title: "Close", onclick: move |_| on_close.call(()), "×" }
            }
            div { class: "modal-body",
                p { class: "settings-hint mcp-lead",
                    "Ask Claude to fill in your store listing: write and translate the store text, caption the \
                     screenshots in every language, bump the version, generate the screenshots and run a build. It \
                     works on the same projects as this window, and what it changes shows up here within a couple of \
                     seconds. Uploading to the stores stays here, in the Submit step."
                }

                p { class: "settings-section-title", "1 · Claude Desktop" }
                p { class: "settings-hint",
                    "Settings \u{2192} Developer \u{2192} Edit Config, paste this in and save, then quit Claude and open \
                     it again. If the file already has an mcpServers section, add just the shipscreen entry to it."
                }
                Snippet { text: desktop.clone() }
                p { class: "settings-hint mcp-path", "The file: {desktop_config_path()}" }

                p { class: "settings-section-title", "2 · Claude Code" }
                p { class: "settings-hint", "Run this once in a terminal:" }
                Snippet { text: code.clone() }

                p { class: "settings-section-title", "3 · Codex" }
                p { class: "settings-hint", "Run this once in a terminal:" }
                Snippet { text: codex.clone() }

                p { class: "settings-section-title", "4 · Try it" }
                p { class: "settings-hint",
                    "Ask: \u{201c}What does my project still need before a release?\u{201d}, or \u{201c}Translate the \
                     store text and captions into French and generate the screenshots.\u{201d}"
                }
                p { class: "settings-hint mcp-note",
                    "Claude in the browser (claude.ai) can\u{2019}t use this: it only connects to servers on the internet, \
                     and ShipScreen runs on your computer. Any other app that speaks MCP can, with the same command: "
                    code { "{command} mcp" }
                }
            }
        }
    }
}

/// A block of text to paste somewhere else, with its own Copy button, as the
/// Accounts step copies values.
#[component]
pub(super) fn Snippet(text: String) -> Element {
    let mut copied = use_signal(|| false);
    let to_copy = text.clone();
    rsx! {
        div { class: "mcp-snippet-wrap",
            pre { class: "mcp-snippet", "{text}" }
            button {
                class: "btn btn-sm mcp-copy",
                onclick: move |_| {
                    let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&to_copy).unwrap_or_default());
                    document::eval(&js);
                    copied.set(true);
                },
                if copied() { "Copied ✓" } else { "Copy" }
            }
        }
    }
}
