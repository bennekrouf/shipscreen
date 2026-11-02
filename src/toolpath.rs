//! Find the developer's tools (`dx`, `cargo`, `rustup`) however ShipScreen was opened.
//!
//! Opened from the Dock or Finder, a macOS app gets only `/usr/bin:/bin:/usr/sbin:/sbin`,
//! so nothing installed by rustup or Homebrew is found. And when Homebrew's deno is
//! installed, its own `dx` can shadow the Dioxus CLI. At startup, before any thread
//! exists, PATH becomes: cargo's bin folder, then the login shell's PATH, then the
//! inherited one. Every command ShipScreen runs, build scripts included, inherits it.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// Rewrites this process's PATH. Call first thing in `main`, before any thread starts.
pub fn init() {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let path = tool_path(
        &current,
        cargo_bin(),
        desktop_kit::shell::login_path().map(OsString::from),
    );
    if path != current {
        tracing::info!("PATH for tools: {}", path.to_string_lossy());
        std::env::set_var("PATH", path);
    }
}

/// `cargo_bin`, then `login`, then `current`, each folder once, in that order.
fn tool_path(current: &OsStr, cargo_bin: Option<PathBuf>, login: Option<OsString>) -> OsString {
    let mut folders: Vec<PathBuf> = Vec::new();
    let candidates = cargo_bin
        .into_iter()
        .chain(
            login
                .iter()
                .flat_map(|p| std::env::split_paths(p).collect::<Vec<_>>()),
        )
        .chain(std::env::split_paths(current));
    for folder in candidates {
        if !folder.as_os_str().is_empty() && !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    std::env::join_paths(folders).unwrap_or_else(|_| current.to_os_string())
}

/// `$CARGO_HOME/bin`, else `~/.cargo/bin`, if it exists.
fn cargo_bin() -> Option<PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))?;
    Some(home.join("bin")).filter(|bin| bin.is_dir())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn cargo_bin_comes_first_then_login_then_inherited() {
        let path = tool_path(
            OsStr::new("/usr/bin:/bin"),
            Some(PathBuf::from("/Users/me/.cargo/bin")),
            Some(OsString::from(
                "/opt/homebrew/bin:/usr/bin:/Users/me/.cargo/bin",
            )),
        );
        assert_eq!(
            path,
            OsString::from("/Users/me/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin")
        );
    }

    #[test]
    fn keeps_inherited_path_when_nothing_else_is_known() {
        assert_eq!(
            tool_path(OsStr::new("/usr/bin:/bin"), None, None),
            OsString::from("/usr/bin:/bin")
        );
    }
}
