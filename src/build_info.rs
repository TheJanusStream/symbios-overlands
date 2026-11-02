//! Which commit this build is.
//!
//! A deployed build is stamped at compile time (`build.rs`, under
//! `SYMBIOS_STAMP_SHA`). A local one is not, because the stamp recompiles the whole
//! crate after every commit, and it says `"unknown"` rather than guess: what git says
//! at run time is the checkout's HEAD, which need not be what this binary was built
//! from, nor even this repository, and a `Hello` carries it to every peer.
//! `SYMBIOS_STAMP_SHA=1 cargo build ...` stamps a local build whose logs must be
//! attributed.

/// The short git sha of this build, or `"unknown"`.
pub fn git_sha() -> &'static str {
    option_env!("SYMBIOS_GIT_SHA").unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_unstamped_build_does_not_guess_from_the_checkout() {
        if option_env!("SYMBIOS_GIT_SHA").is_none() {
            assert_eq!(super::git_sha(), "unknown");
        }
    }
}
