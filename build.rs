//! Build script — captures two facts about the build into the environment.
//!
//! `SYMBIOS_GIT_SHA` is the short git sha, so the diagnostic suite's startup
//! snapshot (src/diagnostics/snapshot.rs) can record which commit produced a
//! session log. Degrades to `"unknown"` outside a git checkout or when `git`
//! is unavailable.
//!
//! `SYMBIOS_AVIAN_VERSION` is the resolved `avian3d` version, so the
//! island-corruption canary in `tests/freeze_rigid_body.rs` can notice when
//! the dependency moves out from under it (#1150). Degrades to `"unknown"`
//! when the lockfile is unreadable.
//!
//! Both are read with `option_env!`, so the crate still builds if this script
//! is ever removed.

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
    //
    // `.git/HEAD` alone is NOT enough: on a branch it holds `ref:
    // refs/heads/<branch>` and does not change when you commit — only the
    // ref file does. Watching just HEAD meant the stamped sha went stale
    // for every commit on a branch, so session logs misattributed their
    // build (a log from a 13-commits-later binary claimed the older sha,
    // which cost real time during the #919 diagnosis). Watch the ref files
    // too; `packed-refs` covers the case where git has packed them away.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    println!("cargo:rustc-env=SYMBIOS_AVIAN_VERSION={}", avian_version());
    println!("cargo:rerun-if-changed=Cargo.lock");
}

/// The resolved `avian3d` version, read straight out of `Cargo.lock`.
///
/// A dependency's version is not otherwise visible to a dependent at compile
/// time — `CARGO_PKG_VERSION` is this package's own, and `DEP_*` exists only
/// for `links` crates — so the lockfile is the only source. It is untracked
/// here (see .gitignore) but cargo always writes it before a build script
/// runs, so this is reading a file that exists by construction rather than by
/// luck. The parse is deliberately dumb: find the `[[package]]` stanza whose
/// name is avian3d, take the `version` line under it.
fn avian_version() -> String {
    let Ok(lock) = std::fs::read_to_string("Cargo.lock") else {
        return "unknown".to_string();
    };
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "name = \"avian3d\"" {
            continue;
        }
        for line in lines.by_ref() {
            if let Some(v) = line.trim().strip_prefix("version = ") {
                return v.trim_matches('"').to_string();
            }
            // A new stanza before a version line means a malformed lockfile;
            // say so rather than reporting the next package's version.
            if line.trim().starts_with("[[") {
                break;
            }
        }
        break;
    }
    "unknown".to_string()
}
