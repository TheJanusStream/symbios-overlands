//! Boot-time parameters supplied via WASM URL query string or native CLI.
//!
//! Picks up an optional destination DID, target spawn position, target spawn
//! yaw, and PDS/relay overrides at app startup. The login UI pre-fills its
//! form from this resource and — when a `did` is supplied — auto-submits, so
//! a shared landmark link drops the recipient straight into the linked
//! overland at the linked pose. See [`build_landmark_link`] for the inverse
//! used by the Diagnostics "Copy Landmark Link" button.
//!
//! On WASM, [`detect`] reads `window.location.search`, parses our params,
//! and scrubs them from the URL bar (preserving `?code=&state=` when an
//! OAuth callback is concurrently in flight) so a subsequent reload does
//! not re-apply the boot params or stray into the URL we shipped to the
//! authorization server. On native it parses `argv` via clap.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Public origin where the WASM build is served. Used as the base URL for
/// landmark links emitted on either target so the link is shareable to
/// anyone with a browser. Mirrors `oauth::WASM_REDIRECT_URI` deliberately
/// — the redirect URI is registered with the authorization server and
/// changes there require a coordinated client-metadata redeploy, so we
/// duplicate the constant here rather than coupling boot params to the
/// OAuth module.
pub const LANDMARK_BASE_URL: &str = "https://thejanusstream.github.io/symbios-overlands";

/// Spawn position. The y component is optional so a hand-typed
/// `pos=x,z` link can mean "drop me here, height from the heightmap"
/// while the round-trip emitted by [`build_landmark_link`] always uses
/// the exact `x,y,z` form.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct TargetPos {
    pub x: f32,
    /// `None` → resolve height from the heightmap at spawn time;
    /// `Some(y)` → use this y exactly.
    pub y: Option<f32>,
    pub z: f32,
}

/// Boot-time configuration captured from the URL query string (WASM) or
/// argv (native). All fields are optional; emptiness is the common case.
#[derive(Resource, Clone, Debug, Default)]
pub struct BootParams {
    pub target_did: Option<String>,
    pub target_pos: Option<TargetPos>,
    pub target_yaw_deg: Option<f32>,
    pub pds: Option<String>,
    pub relay: Option<String>,
    /// True when the boot input contained a `did=` (URL) or `--did` (CLI).
    /// The login UI uses this to auto-submit on the first frame after the
    /// form has been pre-filled — `pds=` / `relay=` alone do not trigger
    /// auto-submit, since they're config without a destination.
    pub autosubmit: bool,
}

impl BootParams {
    /// True when *anything* was supplied. Used to gate the form pre-fill so
    /// a default-empty `BootParams` never overwrites the existing form
    /// defaults.
    pub fn is_any(&self) -> bool {
        self.target_did.is_some()
            || self.target_pos.is_some()
            || self.target_yaw_deg.is_some()
            || self.pds.is_some()
            || self.relay.is_some()
    }
}

/// Parse a `pos=` value. Accepts `x,z` (drop-pin form, y resolved from
/// heightmap) or `x,y,z` (exact). Returns `None` if the string is malformed
/// or any component is non-finite.
pub fn parse_pos(s: &str) -> Option<TargetPos> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    match parts.len() {
        2 => {
            let x: f32 = parts[0].parse().ok()?;
            let z: f32 = parts[1].parse().ok()?;
            (x.is_finite() && z.is_finite()).then_some(TargetPos { x, y: None, z })
        }
        3 => {
            let x: f32 = parts[0].parse().ok()?;
            let y: f32 = parts[1].parse().ok()?;
            let z: f32 = parts[2].parse().ok()?;
            (x.is_finite() && y.is_finite() && z.is_finite())
                .then_some(TargetPos { x, y: Some(y), z })
        }
        _ => None,
    }
}

/// Parse a `rot=` value as yaw in degrees. Rejects NaN and infinities.
pub fn parse_yaw_deg(s: &str) -> Option<f32> {
    let v: f32 = s.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

/// Build the landmark URL for `(did, pos, yaw_deg)`. The output is a
/// fully-qualified HTTPS link to the WASM page; recipients on native can
/// also paste it as `--did=… --pos=… --rot=…` after stripping the host
/// prefix — same param names by design.
pub fn build_landmark_link(did: &str, pos: Vec3, yaw_deg: f32) -> String {
    use url::form_urlencoded::byte_serialize;
    let did_enc: String = byte_serialize(did.as_bytes()).collect();
    format!(
        "{}?did={}&pos={:.2},{:.2},{:.2}&rot={:.1}",
        LANDMARK_BASE_URL, did_enc, pos.x, pos.y, pos.z, yaw_deg
    )
}

// ────────────────────────────────────────────────────────────────────────
// WASM: read window.location.search; scrub our params, preserving code/state
// ────────────────────────────────────────────────────────────────────────

/// Read the URL query string and pop our params into a `BootParams`. Strips
/// the consumed params from the URL bar in a single `history.replaceState`
/// call, leaving any `code=` / `state=` intact for `check_wasm_callback`.
#[cfg(target_arch = "wasm32")]
pub fn detect() -> BootParams {
    let Some(window) = web_sys::window() else {
        return BootParams::default();
    };
    let search = match window.location().search() {
        Ok(s) => s,
        Err(_) => return BootParams::default(),
    };
    let query = search.trim_start_matches('?');
    if query.is_empty() {
        return BootParams::default();
    }

    let mut params = BootParams::default();
    let mut had_oauth_passthrough = false;
    let mut had_our_param = false;

    for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
        match k.as_ref() {
            "did" => {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    params.target_did = Some(trimmed.to_owned());
                    params.autosubmit = true;
                }
                had_our_param = true;
            }
            "pos" => {
                params.target_pos = parse_pos(&v);
                had_our_param = true;
            }
            "rot" => {
                params.target_yaw_deg = parse_yaw_deg(&v);
                had_our_param = true;
            }
            "pds" => {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    params.pds = Some(trimmed.to_owned());
                }
                had_our_param = true;
            }
            "relay" => {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    params.relay = Some(trimmed.to_owned());
                }
                had_our_param = true;
            }
            "code" | "state" => {
                had_oauth_passthrough = true;
            }
            _ => {}
        }
    }

    if had_our_param {
        scrub_our_params(query, had_oauth_passthrough);
    }

    params
}

/// Native build: parse argv via clap. The CLI flags mirror the WASM URL
/// query keys 1:1 so a landmark link can be hand-translated.
#[cfg(not(target_arch = "wasm32"))]
pub fn detect() -> BootParams {
    use clap::Parser;
    let args = CliArgs::parse();
    let mut params = BootParams::default();
    if let Some(did) = args.did.and_then(non_empty) {
        params.target_did = Some(did);
        params.autosubmit = true;
    }
    if let Some(p) = args.pos.as_deref().and_then(parse_pos) {
        params.target_pos = Some(p);
    }
    if let Some(rot) = args.rot.filter(|v| v.is_finite()) {
        params.target_yaw_deg = Some(rot);
    }
    if let Some(pds) = args.pds.and_then(non_empty) {
        params.pds = Some(pds);
    }
    if let Some(relay) = args.relay.and_then(non_empty) {
        params.relay = Some(relay);
    }
    params
}

#[cfg(not(target_arch = "wasm32"))]
fn non_empty(s: String) -> Option<String> {
    let t = s.trim().to_owned();
    (!t.is_empty()).then_some(t)
}

#[cfg(target_arch = "wasm32")]
fn scrub_our_params(query: &str, keep_oauth: bool) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(history) = window.history() else {
        return;
    };
    let mut retained: Vec<(String, String)> = Vec::new();
    if keep_oauth {
        for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
            if k == "code" || k == "state" {
                retained.push((k.into_owned(), v.into_owned()));
            }
        }
    }
    let new_query = if retained.is_empty() {
        String::new()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::from("?"));
        for (k, v) in &retained {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    };
    let target = format!("{}/{}", LANDMARK_BASE_URL, new_query);
    let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&target));
}

// ────────────────────────────────────────────────────────────────────────
// Native: clap argument struct
// ────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Parser, Debug)]
#[command(name = "symbios-overlands", about = "Symbios Overlands client")]
struct CliArgs {
    /// Destination DID (omit for your home overland).
    #[arg(long)]
    did: Option<String>,
    /// Spawn position: `x,z` (height from heightmap) or `x,y,z` (exact).
    #[arg(long, value_name = "X,Z|X,Y,Z")]
    pos: Option<String>,
    /// Spawn yaw in degrees (0 faces -Z; 90 faces +X).
    #[arg(long, value_name = "DEG")]
    rot: Option<f32>,
    /// Override the PDS URL (e.g. `https://bsky.social`).
    #[arg(long, value_name = "URL")]
    pds: Option<String>,
    /// Override the relay host (e.g. `relay.example.com`).
    #[arg(long, value_name = "HOST")]
    relay: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────
// Clipboard
// ────────────────────────────────────────────────────────────────────────

/// Copy `text` to the OS clipboard. Native uses `arboard`; WASM uses the
/// browser's async Clipboard API (call-from-user-gesture only — fine when
/// invoked from an egui button handler).
#[cfg(not(target_arch = "wasm32"))]
pub fn write_to_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard init: {e}"))?;
    cb.set_text(text.to_owned())
        .map_err(|e| format!("clipboard set_text: {e}"))
}

#[cfg(target_arch = "wasm32")]
pub fn write_to_clipboard(text: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let clipboard = window.navigator().clipboard();
    // The returned Promise resolves asynchronously; we don't await it
    // because the egui button click is synchronous. The browser will
    // surface any failure in DevTools; we treat the call as best-effort.
    let _ = clipboard.write_text(text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pos_xz_form() {
        let p = parse_pos("10.5, 20.0").unwrap();
        assert_eq!(p.x, 10.5);
        assert_eq!(p.z, 20.0);
        assert!(p.y.is_none());
    }

    #[test]
    fn parse_pos_xyz_form() {
        let p = parse_pos("1,2,3").unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, Some(2.0));
        assert_eq!(p.z, 3.0);
    }

    #[test]
    fn parse_pos_rejects_nan_and_arity() {
        assert!(parse_pos("nope").is_none());
        assert!(parse_pos("1,2,3,4").is_none());
        assert!(parse_pos("nan,0").is_none());
        assert!(parse_pos("1,inf,3").is_none());
        assert!(parse_pos("").is_none());
    }

    #[test]
    fn parse_yaw_basic() {
        assert_eq!(parse_yaw_deg("180"), Some(180.0));
        assert_eq!(parse_yaw_deg("-90.5"), Some(-90.5));
        assert!(parse_yaw_deg("nan").is_none());
        assert!(parse_yaw_deg("inf").is_none());
        assert!(parse_yaw_deg("foo").is_none());
    }

    #[test]
    fn landmark_link_round_trip() {
        let link = build_landmark_link("did:plc:abc", Vec3::new(10.0, 5.0, -3.0), 90.0);
        assert!(link.contains("did=did%3Aplc%3Aabc"), "link was: {link}");
        assert!(link.contains("pos=10.00,5.00,-3.00"), "link was: {link}");
        assert!(link.contains("rot=90.0"), "link was: {link}");
    }

    #[test]
    fn boot_params_is_any() {
        let mut p = BootParams::default();
        assert!(!p.is_any());
        p.target_yaw_deg = Some(0.0);
        assert!(p.is_any());
    }
}
