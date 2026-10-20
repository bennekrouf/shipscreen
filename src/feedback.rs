//! The way back to mayorana.ch from inside the app.
//!
//! People get ShipScreen as a download and may never see the site again: the
//! update banner and the notice only show when there is something to say.
//! These links are always on screen, so anyone with an idea or a problem has
//! somewhere to send it.

use dioxus::prelude::*;

/// The app's page on the site: releases, docs, the other apps.
const SITE_URL: &str = "https://mayorana.ch/en/apps/shipscreen";

/// The contact form with the topic set and the app and version filled in, so a
/// message says what it is about without the sender having to.
pub fn contact_url() -> String {
    format!(
        "https://mayorana.ch/en/contact?service=tools&app=ShipScreen&v={}",
        env!("CARGO_PKG_VERSION")
    )
}

/// A quiet line of two links, for the projects screen, which has no sidebar.
#[component]
pub fn FeedbackLine() -> Element {
    rsx! {
        div { class: "feedback-line",
            a {
                href: "{contact_url()}",
                target: "_blank",
                title: "Ideas, bugs, questions: send them to the person who makes ShipScreen",
                "Feedback"
            }
            span { " · " }
            a { href: SITE_URL, target: "_blank", "mayorana.ch" }
        }
    }
}
