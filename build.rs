//! Build script - captures one fact about the build into the environment,
//! and tells a web build which browser backend it is for.
//!
//! `SYMBIOS_GIT_SHA` is the short git sha, so a deployed build's session logs
//! and its `Hello` can say which commit produced them. **Only when
//! `SYMBIOS_STAMP_SHA` is set**, as deploy.yml sets it: the variable reaches every
//! target of this crate, so stamping it on every build recompiled all 600,000
//! lines after every commit, 3 to 4 minutes a gate step with no code changed.
//! A local build says `unknown` instead (`crate::build_info`).
//!
//! It is read with `option_env!`, so without this script it would degrade
//! rather than break the build. (`SYMBIOS_AVIAN_VERSION`, the resolved
//! `avian3d` version read out of `Cargo.lock` for the island-corruption
//! canary's version guard, #1150, went with that guard in #1894, when the
//! canary began running in every gate.)
//!
//! The `webgl2` cfg is set on a wasm32 build without the `webgpu` feature
//! (#1670): the WebGL2 browser bundle, which leaves out what that backend
//! cannot draw - MSAA, the depth prepass, the splat's stains overlay. That
//! part is not optional. Without it the WebGL2 bundle would build the other
//! arms and fail in its first frames, and every `cfg(webgl2)` would raise
//! `unexpected_cfgs`, an error under CI's `-D warnings`.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SYMBIOS_STAMP_SHA");
    if std::env::var_os("SYMBIOS_STAMP_SHA").is_some() {
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
        // refs/heads/<branch>` and does not change when you commit - only the
        // ref file does. Watching just HEAD meant the stamped sha went stale
        // for every commit on a branch, so session logs misattributed their
        // build (a log from a 13-commits-later binary claimed the older sha,
        // which cost real time during the #919 diagnosis). Watch the ref files
        // too; `packed-refs` covers the case where git has packed them away.
        println!("cargo:rerun-if-changed=.git/HEAD");
        println!("cargo:rerun-if-changed=.git/refs/heads");
        println!("cargo:rerun-if-changed=.git/packed-refs");
    }

    // `webgl2`: a browser build that draws through WebGL2 (#1670) - wasm32
    // without the `webgpu` feature. What WebGL2 cannot do and WebGPU can
    // (multisampling, a depth texture read in GLSL, a seventeenth texture
    // slot) gates on it, so the WebGPU build lifts those limits while
    // everything that is the web's rather than WebGL2's keeps gating on the
    // target.
    println!("cargo:rustc-check-cfg=cfg(webgl2)");
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|arch| arch == "wasm32");
    let webgpu = std::env::var_os("CARGO_FEATURE_WEBGPU").is_some();
    if wasm && !webgpu {
        println!("cargo:rustc-cfg=webgl2");
    }
}
