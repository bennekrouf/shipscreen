//! ShipScreen's licence. The licence itself (the number, api0's answer, the
//! daily check, the shared file) is `desktop_kit::licence`, the same in every
//! mayorana app; ShipScreen names its product id and where it keeps api0's
//! last answer in `kit`.
//!
//! Without Pro, ShipScreen works on one app (project folder); `pro.rs` keeps
//! which one.

use std::path::PathBuf;

pub use desktop_kit::licence::{current, title_badge, Status};

/// The product id api0 knows ShipScreen by: a plan lists it to make
/// ShipScreen Pro.
pub const PRODUCT: &str = "shipscreen";

/// Where the licence's answer, and the free version's app, are kept.
pub fn dir() -> PathBuf {
    // Beside settings.json, under the name the app had when it was first
    // written (see `global_config_path` in main.rs).
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("appscreens")
}
