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

/// Where the boot params came from (#1227 f250).
///
/// The distinction is the whole of the fix: a `--did` on the command line
/// was typed by the person sitting at the machine, and submitting it
/// without asking is doing what they said. A `?did=` in a URL was written
/// by somebody else and clicked by a stranger who has not yet been told
/// what this app is or whose world the link points at — and on wasm the
/// submit is a full-page navigation to an OAuth consent screen, so the
/// first thing that stranger sees is a third party asking for account
/// access.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BootSource {
    /// A URL query string: a link somebody else wrote.
    #[default]
    Link,
    /// argv on this machine: a human at a shell.
    Cli,
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
    /// True when the boot input contained a `did=` (URL) or `--did` (CLI):
    /// a destination was supplied, so the login form has something to say
    /// about where this session is going. `pds=` / `relay=` alone are
    /// config without a destination and set nothing here.
    ///
    /// This is no longer "submit the form for me" — see [`entry_plan`],
    /// which decides between asking and submitting.
    pub autosubmit: bool,
    /// Which door the params came through ([`BootSource`]).
    pub source: BootSource,
}

/// What the login form should do about a destination it was handed
/// (#1227 f250/f294, #1230 f19).
///
/// Pure, and the one place the decision lives, because it is the same
/// decision reached from four directions: a cold page load, a cold
/// process start, a return to the form after an aborted load, and a
/// return after a logout. Before this the answer was `b.autosubmit`
/// alone, which said yes to all four.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryPlan {
    /// Ordinary form. No destination, or one that has already been used.
    Idle,
    /// Pre-fill and *name* the destination, but the click is the user's.
    Confirm,
    /// Submit without asking.
    Auto,
}

/// Set once this session's boot params have actually carried somebody into
/// a world (#1230 f19).
///
/// App-lifetime, like [`BootParams`] itself, and deliberately NOT part of
/// [`crate::ui::login::LoginUiLatch`]: that latch is reset on
/// `OnEnter(AppState::Login)` so a re-entry behaves like a fresh page
/// load, which is exactly what made the loading screen's abort and Log out
/// re-fire the same flow. Inserted by
/// [`crate::ui::login::complete::install_completed_session`], never
/// removed — a logout tears the session down, not the fact that this
/// process has already spent the link it was started with.
#[derive(Resource, Default, Debug)]
pub struct BootEntrySpent;

/// Decide [`EntryPlan`] (#1227 f250/f294, #1230 f19).
///
/// * `spent` — this session has already carried somebody into a world on
///   these params. Set when a login completes and never cleared, because
///   `AppState::Login` is re-entered by the loading screen's abort and by
///   Log out, and re-firing the flow there was the entire defect in
///   #1230 f19: for a link visitor both escape hatches led straight back
///   into the load they were escaping, so killing the app was the only
///   exit — and Log out did not log out, because the browser bounced off
///   a live IdP session back into the same world.
/// * `has_persisted` — wasm has a saved session; the resume path applies
///   the `did=` override itself and auto-submitting on top would spawn
///   two competing auth tasks.
///
/// A `pds=` or `relay=` override never auto-submits, whatever the source
/// (#1227 f294). Those two parameters choose the OAuth authorization
/// server the browser is navigated to and the relay every chat message,
/// transform and gift envelope of the session flows through; they arrive
/// in the same query string as the destination, and they were rendered
/// inside a collapsed fold. A link that repoints a stranger's
/// infrastructure must not be able to spend their click for them.
pub fn entry_plan(params: &BootParams, spent: bool, has_persisted: bool) -> EntryPlan {
    if !params.autosubmit {
        return EntryPlan::Idle;
    }
    if has_persisted {
        return EntryPlan::Idle;
    }
    let overrides_infrastructure = params.pds.is_some() || params.relay.is_some();
    if params.source == BootSource::Cli && !spent && !overrides_infrastructure {
        return EntryPlan::Auto;
    }
    // Everything else still NAMES the destination and pre-fills the form —
    // a spent link is one click from a retry, which is what #1230 f19 asked
    // to keep. Only the automatic submit is withdrawn.
    EntryPlan::Confirm
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
            (x.is_finite() && y.is_finite() && z.is_finite()).then_some(TargetPos {
                x,
                y: Some(y),
                z,
            })
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

/// What one parse of a query string yielded.
pub struct ParsedQuery {
    pub params: BootParams,
    /// At least one of `did` / `pos` / `rot` / `pds` / `relay` was present,
    /// so the URL bar has something of ours to scrub.
    pub had_our_param: bool,
    /// An OAuth `code=` / `state=` pair is riding along and must survive
    /// the scrub for `check_wasm_callback`.
    pub had_oauth_passthrough: bool,
}

/// Parse a URL query string into [`BootParams`]. Pure, and split out of
/// [`detect`] so the whole of the landmark-link contract — which keys are
/// ours, which set a destination, what an empty value means — is testable
/// without a browser (#1227 f250).
pub fn parse_query(query: &str) -> ParsedQuery {
    let mut params = BootParams {
        source: BootSource::Link,
        ..BootParams::default()
    };
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

    ParsedQuery {
        params,
        had_our_param,
        had_oauth_passthrough,
    }
}

/// Re-encode the params this app owns as a query string, in a fixed key
/// order so a round trip through [`parse_query`] is stable.
///
/// Used to stash the landmark before the URL bar is scrubbed (#1227 f250):
/// `detect` strips `did=`/`pos=`/`rot=` with `history.replaceState`, and a
/// denied or errored OAuth callback drops the pending blob too — so the
/// user landed back on a form that no longer knew where they had been
/// going, with failure copy that never mentioned the lost landmark.
pub fn encode_our_params(params: &BootParams) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(did) = &params.target_did {
        serializer.append_pair("did", did);
    }
    if let Some(pos) = &params.target_pos {
        let encoded = match pos.y {
            Some(y) => format!("{:.2},{:.2},{:.2}", pos.x, y, pos.z),
            None => format!("{:.2},{:.2}", pos.x, pos.z),
        };
        serializer.append_pair("pos", &encoded);
    }
    if let Some(yaw) = params.target_yaw_deg {
        serializer.append_pair("rot", &format!("{yaw:.1}"));
    }
    if let Some(pds) = &params.pds {
        serializer.append_pair("pds", pds);
    }
    if let Some(relay) = &params.relay {
        serializer.append_pair("relay", relay);
    }
    serializer.finish()
}

/// `sessionStorage` key the landmark is stashed under across the OAuth
/// redirect. Session-scoped by design: the round trip stays in one tab, and
/// a stash that outlived the tab would resurrect somebody else's
/// destination on a later visit.
#[cfg(target_arch = "wasm32")]
const BOOT_STASH_KEY: &str = "overlands_boot_params";

/// Read the URL query string and pop our params into a `BootParams`. Strips
/// the consumed params from the URL bar in a single `history.replaceState`
/// call, leaving any `code=` / `state=` intact for `check_wasm_callback`.
///
/// When the URL carries none of ours, the tab's stash is consulted: that is
/// the OAuth return leg, where the authorization server has replaced our
/// query string with its own and a denied login would otherwise land on a
/// blank form (#1227 f250).
#[cfg(target_arch = "wasm32")]
pub fn detect() -> BootParams {
    let Some(window) = web_sys::window() else {
        return BootParams::default();
    };
    let search = window.location().search().unwrap_or_default();
    let query = search.trim_start_matches('?');
    let parsed = parse_query(query);

    if parsed.had_our_param {
        stash_our_params(&parsed.params);
        scrub_our_params(query, parsed.had_oauth_passthrough);
        return parsed.params;
    }
    // No landmark in the URL. Either a plain visit (the stash is empty and
    // this is a no-op) or the OAuth return leg, where restoring it is what
    // lets a denied login say where it had been going.
    match take_stashed_params() {
        Some(stashed) => stashed,
        None => parsed.params,
    }
}

/// Write the landmark into `sessionStorage` before the URL bar loses it.
#[cfg(target_arch = "wasm32")]
fn stash_our_params(params: &BootParams) {
    let encoded = encode_our_params(params);
    if encoded.is_empty() {
        return;
    }
    if let Some(window) = web_sys::window()
        && let Ok(Some(storage)) = window.session_storage()
    {
        let _ = storage.set_item(BOOT_STASH_KEY, &encoded);
    }
}

/// Read the stash back. Left in place rather than removed: the wasm app can
/// re-run `detect` (the boot handoff path reads it before the App exists),
/// and a stash that vanished on first read would leave the second caller
/// with nothing. It dies with the tab either way.
#[cfg(target_arch = "wasm32")]
fn take_stashed_params() -> Option<BootParams> {
    let window = web_sys::window()?;
    let storage = window.session_storage().ok()??;
    let encoded = storage.get_item(BOOT_STASH_KEY).ok()??;
    let parsed = parse_query(&encoded);
    parsed.had_our_param.then_some(parsed.params)
}

/// Native build: parse argv via clap. The CLI flags mirror the WASM URL
/// query keys 1:1 so a landmark link can be hand-translated.
#[cfg(not(target_arch = "wasm32"))]
pub fn detect() -> BootParams {
    use clap::Parser;
    let args = CliArgs::parse();
    let mut params = BootParams {
        source: BootSource::Cli,
        ..BootParams::default()
    };
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

/// A finished clipboard write, waiting to be told to the user.
///
/// `label` is the success wording ("Path copied"); `text` is what was
/// asked for, echoed into the failure toast so it can still be selected
/// by hand when the write did not land.
pub struct ClipboardOutcome {
    pub label: String,
    pub text: String,
    pub result: Result<(), String>,
}

/// Where clipboard writes report what they actually did (#1141).
///
/// Native's `arboard` write finishes before the call returns; the
/// browser's `navigator.clipboard.writeText` returns a **Promise**, and
/// the old wasm path threw it away (`let _ = …`) and returned `Ok(())`.
/// That promise rejects when the document is not focused, when the
/// permission is denied, and when the click was not treated as a user
/// activation — which egui's synthetic frame timing can lose on some
/// browsers. Every caller mapped that `Ok` to a green "Copied: …" toast,
/// so the user pasted nothing after being told it had worked, and the
/// landmark link is the app's primary "come and visit" affordance.
///
/// A queue rather than a return value because the answer is not
/// available on the frame the button was clicked. Both targets push
/// through it so there is one place the wording lives, and
/// [`drain_clipboard_outcomes`] is the only thing that toasts.
#[derive(bevy::prelude::Resource, Default, Clone)]
pub struct ClipboardQueue(std::sync::Arc<std::sync::Mutex<Vec<ClipboardOutcome>>>);

impl ClipboardQueue {
    /// Ask for `text` to be put on the clipboard, reporting the outcome
    /// under `label` once it is known.
    pub fn copy(&self, text: &str, label: &str) {
        let outcome = |result| ClipboardOutcome {
            label: label.to_string(),
            text: text.to_string(),
            result,
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.push(outcome(write_to_clipboard_native(text)));
        }
        #[cfg(target_arch = "wasm32")]
        {
            match wasm_clipboard_promise(text) {
                Err(e) => self.push(outcome(Err(e))),
                Ok(promise) => {
                    // The egui click handler is synchronous, so the only
                    // way to learn whether the browser accepted the write
                    // is to await the promise on the microtask queue and
                    // land the answer in the queue a later frame drains.
                    let sink = self.clone();
                    let outcome_label = label.to_string();
                    let outcome_text = text.to_string();
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = wasm_bindgen_futures::JsFuture::from(promise)
                            .await
                            .map(|_| ())
                            .map_err(|e| {
                                js_sys::Reflect::get(
                                    &e,
                                    &wasm_bindgen::JsValue::from_str("message"),
                                )
                                .ok()
                                .and_then(|m| m.as_string())
                                .unwrap_or_else(|| String::from("the browser refused the write"))
                            });
                        sink.push(ClipboardOutcome {
                            label: outcome_label,
                            text: outcome_text,
                            result,
                        });
                    });
                }
            }
        }
    }

    fn push(&self, outcome: ClipboardOutcome) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push(outcome);
        }
    }

    /// Take everything reported since the last drain.
    pub fn take(&self) -> Vec<ClipboardOutcome> {
        self.0
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

/// Put `text` on the OS clipboard via `arboard`. Synchronous: by the time
/// this returns the write has either landed or failed.
#[cfg(not(target_arch = "wasm32"))]
fn write_to_clipboard_native(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard init: {e}"))?;
    cb.set_text(text.to_owned())
        .map_err(|e| format!("clipboard set_text: {e}"))
}

/// Start a browser clipboard write and hand back its Promise, or the
/// reason no write could be started at all.
#[cfg(target_arch = "wasm32")]
fn wasm_clipboard_promise(text: &str) -> Result<js_sys::Promise, String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let navigator = window.navigator();
    // `navigator.clipboard` is undefined outside secure contexts (plain
    // HTTP, sandboxed iframes without clipboard permission). web-sys
    // projects the property as always-present, and calling `write_text`
    // through an undefined reference throws a JS TypeError that unwinds
    // straight through the Bevy frame loop — probe for it first.
    let clipboard_prop = js_sys::Reflect::get(
        navigator.as_ref(),
        &wasm_bindgen::JsValue::from_str("clipboard"),
    )
    .map_err(|_| "clipboard probe failed".to_string())?;
    if clipboard_prop.is_undefined() || clipboard_prop.is_null() {
        return Err("Clipboard API unavailable (insecure context or sandboxed iframe)".to_string());
    }
    Ok(navigator.clipboard().write_text(text))
}

/// Toast whatever the clipboard writes turned out to have done.
///
/// The success wording is the caller's `label`; a failure names the
/// reason **and** repeats the text, because a copy that did not land
/// leaves the user with nothing else to work from.
pub fn drain_clipboard_outcomes(
    queue: bevy::prelude::Res<ClipboardQueue>,
    mut toasts: bevy::prelude::ResMut<crate::ui::toast::Toasts>,
    time: bevy::prelude::Res<bevy::prelude::Time>,
) {
    let now = time.elapsed_secs_f64();
    for outcome in queue.take() {
        match outcome.result {
            Ok(()) => toasts.success(outcome.label, now),
            Err(reason) => toasts.error(format!("Copy failed ({reason}) — {}", outcome.text), now),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// File download (WASM)
// ────────────────────────────────────────────────────────────────────────

/// Trigger a browser "save file" download of `contents` under `filename`
/// (WASM only — native builds write the same data straight to disk). Wraps the
/// string in an in-memory `Blob`, mints an object URL, wires it to a hidden
/// `<a download>`, and synthesises a click — the standard "export to file" web
/// idiom, and the download-log counterpart to [`write_to_clipboard`]. Must be
/// called from a user-gesture handler (an egui button click qualifies). The
/// object URL is revoked on a *deferred* timer, not synchronously: the browser
/// reads the blob on a task scheduled after `click()` returns, so an immediate
/// revoke can tear the `blob:` URL down before its bytes are read and produce an
/// empty file (Firefox bug 1282407 — the same reason FileSaver.js defers it).
#[cfg(target_arch = "wasm32")]
pub fn download_text_file(filename: &str, mime: &str, contents: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let document = window.document().ok_or_else(|| "no document".to_string())?;

    // Blob from a single string part, tagged with a text MIME so a browser that
    // ignores the `download` hint still treats it as text rather than binary.
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(contents));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
        .map_err(|_| "blob create failed".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "object URL create failed".to_string())?;

    // A hidden `<a href=blob download=filename>` click drives the save dialog.
    let anchor = document
        .create_element("a")
        .map_err(|_| "anchor create failed".to_string())?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "anchor cast failed".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);

    // Firefox only fires the download for a synthetic click when the anchor is
    // actually in the document; attach it (hidden), click, then remove it.
    if let Some(body) = document.body() {
        let _ = body.append_child(&anchor);
        anchor.click();
        let _ = body.remove_child(&anchor);
    } else {
        // No <body> (shouldn't happen in a rendered app) — try the detached
        // click, which Chromium honours.
        anchor.click();
    }

    // Defer the revoke: the download task that reads the blob is scheduled
    // *after* this call returns, so revoking now can race it to an empty file.
    // A one-shot timer (self-freeing `once_into_js` closure) keeps the URL alive
    // long enough; 60 s comfortably covers a slow "Save As" prompt. If the timer
    // can't be scheduled, revoke inline rather than leak the blob URL.
    let url_for_revoke = url.clone();
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url_for_revoke);
    });
    if window
        .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 60_000)
        .is_err()
    {
        let _ = web_sys::Url::revoke_object_url(&url);
    }
    Ok(())
}

#[cfg(test)]
mod landmark_link_tests {
    use super::*;

    /// #1227 f250. The landmark-link contract, which was untestable while
    /// it lived inside a `web_sys` call: which keys are ours, which of them
    /// arms a destination, and what an empty value means.
    #[test]
    fn a_landmark_link_yields_a_destination_a_pose_and_a_source() {
        let parsed = parse_query("did=did%3Aplc%3Afriend&pos=1.50,2.00,3.50&rot=90.0");
        assert!(parsed.had_our_param);
        assert!(!parsed.had_oauth_passthrough);
        assert_eq!(parsed.params.target_did.as_deref(), Some("did:plc:friend"));
        assert_eq!(
            parsed.params.target_pos,
            Some(TargetPos {
                x: 1.5,
                y: Some(2.0),
                z: 3.5
            })
        );
        assert_eq!(parsed.params.target_yaw_deg, Some(90.0));
        assert!(parsed.params.autosubmit, "a did= is a destination");
        assert_eq!(
            parsed.params.source,
            BootSource::Link,
            "a query string is somebody else's link, whatever it contains"
        );
    }

    /// An empty `did=` is not a destination — it must not arm anything —
    /// but it is still ours, so it is still scrubbed from the URL bar.
    #[test]
    fn an_empty_destination_arms_nothing_and_is_still_ours() {
        let parsed = parse_query("did=&pds=");
        assert!(parsed.had_our_param);
        assert_eq!(parsed.params.target_did, None);
        assert!(!parsed.params.autosubmit);
        assert_eq!(parsed.params.pds, None);
    }

    /// The OAuth return leg: `code=`/`state=` must survive the scrub, and
    /// none of it is ours.
    #[test]
    fn an_oauth_callback_is_recognised_and_owns_none_of_our_keys() {
        let parsed = parse_query("code=abc&state=xyz");
        assert!(parsed.had_oauth_passthrough);
        assert!(!parsed.had_our_param);
        assert!(!parsed.params.autosubmit);
    }

    /// #1227 f250, the half the refuter confirmed exactly: `detect` scrubs
    /// `did=`/`pos=`/`rot=` from the URL bar, and a denied callback drops
    /// the pending blob — so the user landed back on a form that no longer
    /// knew where they had been going. The stash is what survives that, and
    /// it only works if it round-trips.
    #[test]
    fn a_stashed_landmark_comes_back_intact() {
        let original = parse_query(
            "did=did%3Aplc%3Afriend&pos=10.25,-4.50&rot=-33.5             &pds=https%3A%2F%2Fpds.example&relay=relay.example",
        )
        .params;
        let restored = parse_query(&encode_our_params(&original)).params;
        assert_eq!(restored.target_did, original.target_did);
        assert_eq!(restored.target_pos, original.target_pos);
        assert_eq!(restored.target_yaw_deg, original.target_yaw_deg);
        assert_eq!(restored.pds, original.pds);
        assert_eq!(restored.relay, original.relay);
        assert!(restored.autosubmit);
        // A drop-pin pose (no y) must not come back as an exact one at y=0:
        // the height is resolved from the destination's heightmap, and a
        // literal 0 would park the arrival under the ground.
        assert_eq!(restored.target_pos.and_then(|p| p.y), None);
    }

    /// Nothing to stash stays nothing — an empty encode must not write a
    /// stash that a later plain visit would read back as a destination.
    #[test]
    fn an_empty_boot_encodes_to_nothing() {
        assert!(encode_our_params(&BootParams::default()).is_empty());
        assert!(!parse_query("").had_our_param);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A copy the clipboard refused says so, and hands the text back**
    /// (#1141).
    ///
    /// The wasm path used to discard `writeText`'s Promise and return
    /// `Ok(())`, so every call site toasted a green "Copied: …" whether
    /// or not the write landed — and `navigator.clipboard.writeText`
    /// rejects on a document that is not focused, on a denied permission,
    /// and when the click was not treated as a user activation. Asserting
    /// on the toast the person reads, through the real system, because
    /// that string is the whole defect: a silent failure is recoverable,
    /// a false success is not.
    #[test]
    fn a_refused_copy_is_reported_with_the_text_that_did_not_land() {
        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.init_resource::<ClipboardQueue>();
        app.init_resource::<crate::ui::toast::Toasts>();
        app.add_systems(bevy::prelude::Update, drain_clipboard_outcomes);

        let queue = app.world().resource::<ClipboardQueue>().clone();
        queue.push(ClipboardOutcome {
            label: String::from("Copied: https://example/room"),
            text: String::from("https://example/room"),
            result: Err(String::from("Document is not focused")),
        });
        queue.push(ClipboardOutcome {
            label: String::from("Path copied"),
            text: String::from("/tmp/session.log"),
            result: Ok(()),
        });
        app.update();

        let toasts = app.world().resource::<crate::ui::toast::Toasts>();
        let shown = toasts.shown();
        assert_eq!(shown.len(), 2, "one toast per outcome: {shown:?}");
        assert_eq!(shown[0].0, crate::ui::toast::ToastKind::Error);
        assert!(
            shown[0].1.contains("Document is not focused"),
            "the failure names the browser's reason: {:?}",
            shown[0].1
        );
        assert!(
            shown[0].1.contains("https://example/room"),
            "and repeats the text, since the user now has nothing else: {:?}",
            shown[0].1
        );
        assert_eq!(shown[1].0, crate::ui::toast::ToastKind::Success);
        assert_eq!(shown[1].1, "Path copied");

        // Drained, not re-read: a second frame must not re-toast.
        app.update();
        let shown = app.world().resource::<crate::ui::toast::Toasts>().shown();
        assert_eq!(shown.len(), 2, "outcomes are taken, not peeked: {shown:?}");
    }

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
