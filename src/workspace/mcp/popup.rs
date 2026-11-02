//! The window that shows how to use ShipScreen from Claude.

use super::*;

#[component]
pub(crate) fn McpPopup(on_close: EventHandler<()>) -> Element {
    rsx! {
        desktop_kit::ui::ConnectWindow {
            lead: "Ask Claude to fill in your store listing: write and translate the store text, caption the \
                   screenshots in every language, bump the version, generate the screenshots and run a build. \
                   It works on the same projects as this window, and what it changes shows up here within a \
                   couple of seconds. Uploading to the stores stays here, in the Submit step.",
            try_it: "\u{201c}What does my project still need before a release?\u{201d}, or \u{201c}Translate the \
                     store text and captions into French and generate the screenshots.\u{201d}",
            on_close,
        }
    }
}
