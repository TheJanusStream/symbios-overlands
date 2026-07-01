//! Build script — captures the short git sha into `SYMBIOS_GIT_SHA` so the
//! diagnostic suite's startup snapshot (src/diagnostics/snapshot.rs) can record
//! which commit produced a session log. Degrades to `"unknown"` outside a git
//! checkout or when `git` is unavailable; `snapshot.rs` reads it via
//! `option_env!`, so the crate still builds if this script is ever removed.

use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=SYMBIOS_GIT_SHA={sha}");
    // Re-run when HEAD moves so the sha stays current without a clean rebuild.
    println!("cargo:rerun-if-changed=.git/HEAD");
}
