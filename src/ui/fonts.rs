//! Font pass (#858): bundled base font + lazily-loaded CJK fallback.
//!
//! egui's embedded fonts cover Latin adequately but hold zero CJK
//! glyphs, so a Chinese/Japanese/Korean chat message or profile string
//! rendered as tofu. The fix has two halves:
//!
//! * **Base font** — Noto Sans Regular (Latin/Cyrillic/Greek, ~600 KB)
//!   is compiled in via `include_bytes!` and installed at startup as
//!   the primary proportional font, with egui's embedded fonts kept as
//!   fallback tail ([`build_font_definitions`]).
//! * **CJK fallback** — Noto Sans CJK SC (~16 MB) is far too heavy to
//!   compile in (it would triple the wasm download), so it ships as a
//!   plain asset (`assets/fonts/`) and loads lazily: the first time a
//!   CJK code point is sighted ([`detect_script_needs`]), the file is
//!   read (native) or fetched from the deploy origin (wasm) on the
//!   `IoTaskPool`, and [`poll_cjk_fetch`] swaps in a rebuilt
//!   `FontDefinitions` once, per the 2026-07-17 lazy-fetch decision.
//!
//! `ctx.set_fonts` is a full atlas swap, so the state machine
//! ([`CjkFonts`]) guarantees it happens at most twice per session
//! (base install, CJK upgrade) — never per frame.
//!
//! **"Brief tofu-then-correct" is the native account of the CJK load.**
//! On wasm it is a ~16 MB download raced against a three-minute timeout,
//! which on a slow link is neither brief nor obviously a font at all, and
//! a missing asset leaves the session in a terminal `Failed`. None of
//! that is worth blocking a session over — but it is worth *saying*, so
//! the transitions reach the toast channel through [`surface_font_status`]
//! rather than the log alone (#1262 f361).
//!
//! ## What this module cannot do
//!
//! **No face for Hebrew, Arabic, Thai or the Indic scripts.** The base
//! font is Latin/Cyrillic/Greek, egui's embedded tail adds Ubuntu-Light
//! and two emoji faces, and the one fetchable fallback is CJK — so those
//! scripts are tofu for the whole session with no trigger that could
//! change it. Shipping the faces is an asset-weight decision for the
//! owner, not something this module can take on its own; what it does
//! instead is *say so*, once per script, from [`UNSUPPORTED_SCRIPTS`].
//!
//! **No bidi, so do not ship an RTL face without reading this.** epaint
//! 0.35 shapes through harfrust and guesses segment properties, so a
//! single-script Arabic or Hebrew run joins and orders correctly once a
//! covering face exists — but there is no bidirectional *reordering*
//! across runs (the TODO is epaint's own, at `text/font.rs:830`, and
//! `text_layout.rs` records that run segmentation "would need
//! script-aware splitting once RTL/bidi support is added"). A mixed
//! line — an Arabic name beside a Latin handle or a digit — therefore
//! comes out in logical segment order, which is wrong. Adding an RTL
//! face without fixing that trades empty boxes for confidently wrong
//! text, which is harder for a reader to diagnose, not easier. Whoever
//! adds one should wrap user-supplied strings in isolate marks
//! (U+2068 … U+2069) at the interpolation sites first. Not done here:
//! with no covering face the marks would isolate nothing, and dead
//! machinery is how a limitation gets forgotten (#1262 f368).
//!
//! **Only Simplified-Chinese glyph forms, ever.** The single fetchable
//! face is the SC regional cut, so Japanese and Korean text is readable
//! but drawn in Chinese letterforms for the several hundred unified
//! ideographs whose shapes differ. Shipping the JP and KR cuts as well
//! is ~48 MB of assets for a papercut; the choice is recorded in
//! `assets/fonts/README.md` so it stays a decision (#1262 f373).

use bevy::prelude::*;
use bevy::tasks::Task;
use bevy_egui::{EguiContexts, egui};

/// Noto Sans Regular, compiled in. OFL-1.1 — see `assets/fonts/README.md`.
const BASE_FONT: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");

/// Runtime path of the CJK fallback, relative to the app root on both
/// targets (native: the working directory; wasm: the deploy origin,
/// resolved against `window.location` by `cjk_font_url` — a plain
/// code reference, as that helper only exists on wasm builds).
const CJK_FONT_ASSET_PATH: &str = "assets/fonts/NotoSansCJKsc-Regular.otf";

/// Wall-clock cap on the wasm font fetch. Generous — the OTF is ~16 MB
/// and a slow link is still worth waiting out — but bounded, because
/// browser reqwest has no builder timeout and a hung fetch would pin
/// the state machine in `Fetching` forever (same rationale as #849's
/// record-fetch race).
#[cfg(target_arch = "wasm32")]
const CJK_FETCH_TIMEOUT_SECS: u64 = 180;

/// Lazy-CJK state machine. At most one fetch per session; `Failed` is
/// terminal (a retry loop against a missing asset would just spam the
/// network/log — the operator fixes the deploy and the next session
/// picks it up).
#[derive(Default)]
pub enum CjkStatus {
    /// No CJK text seen yet — nothing loaded.
    #[default]
    Dormant,
    /// CJK text seen; the font bytes are on their way.
    Fetching,
    /// The rebuilt `FontDefinitions` (base + CJK tail) are live.
    Installed,
    /// The load failed; tofu stays for this session.
    Failed,
}

/// Resource owning the CJK lazy-load: status + the in-flight task.
#[derive(Resource, Default)]
pub struct CjkFonts {
    pub status: CjkStatus,
    task: Option<Task<Result<Vec<u8>, String>>>,
}

/// True if `text` contains a code point our bundled base font cannot
/// draw but the CJK fallback can: the unified ideograph blocks, kana,
/// hangul, CJK punctuation and full-width forms.
fn needs_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(u32::from(c),
            0x3000..=0x303F   // CJK punctuation
            | 0x3040..=0x30FF // hiragana + katakana
            | 0x31F0..=0x31FF // katakana phonetic extensions
            | 0x3400..=0x4DBF // CJK ext A
            | 0x4E00..=0x9FFF // CJK unified
            | 0xAC00..=0xD7AF // hangul syllables
            | 0xF900..=0xFAFF // CJK compatibility
            | 0xFE30..=0xFE4F // CJK compat forms
            | 0xFF00..=0xFFEF // full-width forms
        )
    })
}

/// A script the app has no face for, and no code path that could load
/// one (#1262 f360).
///
/// The bundled base font is Latin/Cyrillic/Greek and the only fallback
/// that can ever be fetched is the CJK OTF, so every script listed here
/// is tofu for the whole session. `sample` is probe data for
/// `the_unsupported_script_table_names_real_gaps`, never drawn — the
/// guard asks the real charmaps whether the gap is still a gap, so a
/// font change that closes one fails the test instead of leaving a
/// message that lies to the user.
struct ScriptGap {
    /// What the user is told their text is written in.
    name: &'static str,
    /// One representative code point, probed against the base atlas.
    ///
    /// Test-only by design: it exists so the guard can ask the real
    /// charmaps whether this row is still true, and a deliberately
    /// *assigned letter* is the probe — deriving one from the block's low
    /// bound would often land on an unassigned or combining code point,
    /// which no face owns and which would therefore pass vacuously.
    #[cfg_attr(not(test), allow(dead_code))]
    sample: char,
    /// The Unicode blocks that script writes in, as inclusive pairs.
    blocks: &'static [(u32, u32)],
}

/// Scripts with neither a bundled face nor a fetchable one.
///
/// Not exhaustive and does not claim to be: it covers the scripts f360
/// named plus their immediate neighbours, which is what the toast needs
/// in order to name what it cannot draw. Adding a row costs nothing but
/// a probe; the guard refuses a row whose gap has since closed.
const UNSUPPORTED_SCRIPTS: &[ScriptGap] = &[
    ScriptGap {
        name: "Hebrew",
        sample: '\u{05D0}',
        blocks: &[(0x0590, 0x05FF), (0xFB1D, 0xFB4F)],
    },
    ScriptGap {
        name: "Arabic",
        sample: '\u{0627}',
        blocks: &[
            (0x0600, 0x06FF),
            (0x0750, 0x077F),
            (0x08A0, 0x08FF),
            (0xFB50, 0xFDFF),
            (0xFE70, 0xFEFF),
        ],
    },
    ScriptGap {
        name: "Syriac",
        sample: '\u{0710}',
        blocks: &[(0x0700, 0x074F)],
    },
    ScriptGap {
        name: "Thaana",
        sample: '\u{0780}',
        blocks: &[(0x0780, 0x07BF)],
    },
    ScriptGap {
        name: "Armenian",
        sample: '\u{0531}',
        blocks: &[(0x0530, 0x058F)],
    },
    ScriptGap {
        name: "Georgian",
        sample: '\u{10D0}',
        blocks: &[(0x10A0, 0x10FF), (0x1C90, 0x1CBF)],
    },
    ScriptGap {
        name: "Devanagari",
        sample: '\u{0905}',
        blocks: &[(0x0900, 0x097F), (0xA8E0, 0xA8FF)],
    },
    ScriptGap {
        name: "Bengali",
        sample: '\u{0985}',
        blocks: &[(0x0980, 0x09FF)],
    },
    ScriptGap {
        name: "Gurmukhi",
        sample: '\u{0A05}',
        blocks: &[(0x0A00, 0x0A7F)],
    },
    ScriptGap {
        name: "Gujarati",
        sample: '\u{0A85}',
        blocks: &[(0x0A80, 0x0AFF)],
    },
    ScriptGap {
        name: "Odia",
        sample: '\u{0B05}',
        blocks: &[(0x0B00, 0x0B7F)],
    },
    ScriptGap {
        name: "Tamil",
        sample: '\u{0B85}',
        blocks: &[(0x0B80, 0x0BFF)],
    },
    ScriptGap {
        name: "Telugu",
        sample: '\u{0C05}',
        blocks: &[(0x0C00, 0x0C7F)],
    },
    ScriptGap {
        name: "Kannada",
        sample: '\u{0C85}',
        blocks: &[(0x0C80, 0x0CFF)],
    },
    ScriptGap {
        name: "Malayalam",
        sample: '\u{0D05}',
        blocks: &[(0x0D00, 0x0D7F)],
    },
    ScriptGap {
        name: "Sinhala",
        sample: '\u{0D85}',
        blocks: &[(0x0D80, 0x0DFF)],
    },
    ScriptGap {
        name: "Thai",
        sample: '\u{0E01}',
        blocks: &[(0x0E00, 0x0E7F)],
    },
    ScriptGap {
        name: "Lao",
        sample: '\u{0E81}',
        blocks: &[(0x0E80, 0x0EFF)],
    },
    ScriptGap {
        name: "Tibetan",
        sample: '\u{0F40}',
        blocks: &[(0x0F00, 0x0FFF)],
    },
    ScriptGap {
        name: "Myanmar",
        sample: '\u{1000}',
        blocks: &[(0x1000, 0x109F)],
    },
    ScriptGap {
        name: "Ethiopic",
        sample: '\u{1200}',
        blocks: &[(0x1200, 0x137F)],
    },
    ScriptGap {
        name: "Khmer",
        sample: '\u{1780}',
        blocks: &[(0x1780, 0x17FF)],
    },
];

/// The name of the first unsupported script `text` is written in, if
/// any (#1262 f360).
fn unsupported_script(text: &str) -> Option<&'static str> {
    text.chars().find_map(|c| {
        let cp = u32::from(c);
        UNSUPPORTED_SCRIPTS
            .iter()
            .find(|gap| gap.blocks.iter().any(|(lo, hi)| (*lo..=*hi).contains(&cp)))
            .map(|gap| gap.name)
    })
}

/// Build the app's font set: Noto Sans primary, egui's embedded fonts
/// as tail, plus — once loaded — the CJK fallback at the very end of
/// both families.
pub(crate) fn build_font_definitions(cjk: Option<Vec<u8>>) -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    defs.font_data.insert(
        "noto-sans".to_owned(),
        egui::FontData::from_static(BASE_FONT).into(),
    );
    if let Some(family) = defs.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "noto-sans".to_owned());
    }
    if let Some(family) = defs.families.get_mut(&egui::FontFamily::Monospace) {
        // Fallback only: egui's embedded monospace face keeps priority.
        family.push("noto-sans".to_owned());
    }
    if let Some(bytes) = cjk {
        defs.font_data.insert(
            "noto-cjk".to_owned(),
            egui::FontData::from_owned(bytes).into(),
        );
        for family_name in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(family) = defs.families.get_mut(&family_name) {
                family.push("noto-cjk".to_owned());
            }
        }
    }
    defs
}

/// Size of `TextStyle::Small`, raised from egui's stock 9.0 (#1259
/// f243).
///
/// 9 pt was the app's floor and it was reserved for exactly the text a
/// confused user most needs to read: the whole toast body — the only
/// success/failure channel there is — the login *Details* disclosure
/// carrying the raw error chain, anomaly descriptions tinted with the
/// severity ramp, the peer build-incompatibility chip and the avatar
/// recovery banner. There are ~100 `.small()` call sites and the three
/// load-bearing ones are promoted to Body outright; this raises the
/// floor under all the rest.
///
/// 11 and not 13: `Small` still has to READ as a quieter tier beside
/// Body, or every timestamp and unit suffix starts competing with the
/// text it annotates. It scales with the #1259 f239 UI-scale control on
/// top of this, since `zoom_factor` multiplies point sizes.
pub const SMALL_TEXT_SIZE: f32 = 11.0;

/// Install the base font set at startup. Same self-retrying latch shape
/// as `theme::apply_theme_on_change`: the egui context may not exist on
/// the first frame, and this must not silently give up.
pub fn install_base_fonts(mut contexts: EguiContexts, mut installed: Local<bool>) {
    if *installed {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    ctx.set_fonts(build_font_definitions(None));
    // Both bases: the app pins `theme_preference` per palette
    // (`theme::apply_theme`), so a light palette reads the light `Style`
    // and a dark one the dark. `Context::set_visuals` assigns
    // `style.visuals` alone, so the theme picker cannot undo this.
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(SMALL_TEXT_SIZE, egui::FontFamily::Proportional),
        );
    });
    *installed = true;
}

/// Scripts already reported to the user this session (#1262 f360).
///
/// One message per script, ever: the point is to tell someone once that
/// their writing system is not supported, not to interrupt them every
/// time a peer says something in it.
#[derive(Resource, Default)]
pub struct ScriptGaps {
    reported: std::collections::HashSet<&'static str>,
}

/// A sighting written by a draw site and drained by
/// [`detect_script_needs`] (#1262 f359).
#[derive(Clone, Default)]
struct DrawnSighting {
    cjk: bool,
    gap: Option<&'static str>,
}

/// egui temp-memory slot the drawn-text channel lives in.
fn sighting_id() -> egui::Id {
    egui::Id::new("symbios-font-sighting")
}

/// Report text a UI surface is about to draw, so the font machinery can
/// see the strings that never reach a resource (#1262 f359).
///
/// The detector below reads change-gated ECS sources, which covers text
/// that has been *committed* somewhere. It cannot see a half-typed name
/// in a deferred-commit field (`room::widgets::text_draft_row`
/// keeps its draft in egui's own temp memory), and scanning the whole
/// live room record instead would mean walking every authored name on
/// every frame of a gizmo drag. So a draw site says what it is drawing,
/// through egui's memory rather than a `SystemParam`, because the
/// alternative is a new resource threaded into every UI system in the
/// crate and this repo has a 16-parameter ceiling it has hit before.
///
/// **Where to call it:** any surface that draws a string a user typed
/// and that is not already one of the detector's arms. Cost is a scan of
/// the string; nothing is written unless it actually contains text the
/// fonts cannot draw, which is the rare case.
pub fn note_drawn_text(ctx: &egui::Context, text: &str) {
    let cjk = needs_cjk(text);
    let gap = unsupported_script(text);
    if !cjk && gap.is_none() {
        return;
    }
    ctx.data_mut(|d| {
        let mut seen = d
            .get_temp::<DrawnSighting>(sighting_id())
            .unwrap_or_default();
        seen.cjk |= cjk;
        seen.gap = seen.gap.or(gap);
        d.insert_temp(sighting_id(), seen);
    });
}

/// Read and clear the drawn-text channel.
fn take_drawn_sighting(ctx: &egui::Context) -> DrawnSighting {
    ctx.data_mut(|d| {
        let seen = d
            .get_temp::<DrawnSighting>(sighting_id())
            .unwrap_or_default();
        d.insert_temp(sighting_id(), DrawnSighting::default());
        seen
    })
}

/// Every string a chat history offers the font detector.
///
/// The draft comes FIRST and is the whole point (#1262 f359): the scan
/// used to read `messages` alone, so a sentence being typed in Japanese
/// was a row of empty boxes in the sender's own input field until they
/// pressed Send and it became a message. The draft lives on this same
/// resource ([`crate::state::ChatHistory::draft`], #1140), so it was one
/// line away the entire time.
fn chat_sources(chat: &crate::state::ChatHistory) -> impl Iterator<Item = &str> {
    std::iter::once(chat.draft.as_str()).chain(
        chat.messages
            .iter()
            .flat_map(|m| [m.text.as_str(), m.author.as_str()]),
    )
}

/// Watch every string that can carry arbitrary user text and kick off
/// the CJK load the first time one needs it; report a script the app
/// cannot draw at all (#1262 f359/f360).
///
/// Two kinds of source, and which one a surface belongs to is a cost
/// question. **Change-gated ECS arms** carry text that is small and
/// rarely rewritten — the chat scrollback and its draft, the login feed,
/// peer handles, a mutuals listing, the one open gift dialog, the item
/// names in the stash. **The drawn-text channel**
/// ([`note_drawn_text`]) carries everything else: drafts that live in
/// egui's memory rather than in a resource, and names inside the live
/// room record, which changes every frame of a gizmo drag and would make
/// a record-wide scan a permanent per-frame cost. The channel is bounded
/// by what is on screen, which is the right bound.
///
/// The CJK half latches — once the fetch starts there is nothing left to
/// detect — while the script-gap half keeps watching, because a second
/// unsupported script can turn up at any point in a session.
#[allow(clippy::too_many_arguments)]
pub fn detect_script_needs(
    mut contexts: EguiContexts,
    mut cjk: ResMut<CjkFonts>,
    mut gaps: ResMut<ScriptGaps>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    time: Res<Time>,
    chat: Res<crate::state::ChatHistory>,
    feed: Res<crate::ui::login::LoginPostFeed>,
    mutuals: Res<crate::social::MutualsCache>,
    offer: Option<Res<crate::state::IncomingOfferDialog>>,
    inventory: Option<Res<crate::state::LiveInventoryRecord>>,
    changed_peers: Query<&crate::state::RemotePeer, Changed<crate::state::RemotePeer>>,
) {
    let want_cjk = matches!(cjk.status, CjkStatus::Dormant);

    // Drained first, and unconditionally: leaving a sighting in egui's
    // memory would have it re-reported on every later frame.
    let drawn = contexts
        .ctx_mut()
        .map(|ctx| take_drawn_sighting(ctx))
        .unwrap_or_default();
    let mut hit_cjk = want_cjk && drawn.cjk;
    let mut gap = drawn.gap;

    {
        let mut see = |text: &str| {
            if want_cjk && !hit_cjk {
                hit_cjk = needs_cjk(text);
            }
            if gap.is_none() {
                gap = unsupported_script(text);
            }
        };

        if chat.is_changed() {
            for text in chat_sources(&chat) {
                see(text);
            }
        }
        if feed.is_changed() {
            for post in &feed.posts {
                see(&post.text);
                see(&post.author_handle);
            }
        }
        if mutuals.is_changed() {
            for cached in mutuals.by_owner.values() {
                if let crate::social::MutualsState::Ready(list) = &cached.state {
                    for entry in &list.mutuals {
                        see(&entry.handle);
                        if let Some(name) = &entry.display_name {
                            see(name);
                        }
                    }
                }
            }
        }
        if let Some(offer) = offer.as_ref().filter(|o| o.is_changed()) {
            see(&offer.item_name);
            see(&offer.sender_label.name());
        }
        if let Some(inventory) = inventory.as_ref().filter(|i| i.is_changed()) {
            for item_name in inventory.0.generators.keys() {
                see(item_name);
            }
        }
        for peer in changed_peers.iter() {
            if let Some(handle) = peer.handle.as_deref() {
                see(handle);
            }
        }
    }

    if hit_cjk {
        info!("CJK text sighted — loading the CJK font fallback");
        cjk.status = CjkStatus::Fetching;
        cjk.task = Some(spawn_cjk_load());
    }

    // `contains` before `insert` so a repeated sighting does not deref
    // the resource mutably and mark it changed every frame (#879).
    if let Some(name) = gap
        && !gaps.reported.contains(name)
    {
        gaps.reported.insert(name);
        warn!("{name} text sighted; no bundled or fetchable face covers it — it renders as tofu");
        toasts.warn(
            format!("{name} text can't be shown — this app bundles no font for that script."),
            time.elapsed_secs_f64(),
        );
    }
}

/// What the user should be told about a font-state transition (#1262
/// f361), or `None` for a transition that needs no message.
///
/// `slow_load` is whether fetching the face is something a user would
/// notice: on wasm it is a ~16 MB download over the network, on native a
/// local file read that finishes before the next frame. Announcing a
/// load that has already finished is worse than saying nothing, so the
/// "loading" message is the slow target's only. A parameter rather than
/// a `cfg!` inside the body so both answers are reachable from a test on
/// either target.
pub(crate) fn status_toast(
    status: &CjkStatus,
    slow_load: bool,
) -> Option<(crate::ui::toast::ToastKind, &'static str)> {
    match status {
        CjkStatus::Fetching if slow_load => Some((
            crate::ui::toast::ToastKind::Info,
            "Loading the font for this text — it is a large download and may take a moment.",
        )),
        CjkStatus::Failed => Some((
            crate::ui::toast::ToastKind::Warn,
            "Some text can't be displayed — the font for it failed to load. It will stay as \
             empty boxes until you reload.",
        )),
        _ => None,
    }
}

/// Put the CJK font's lifecycle on a surface the user can see (#1262
/// f361).
///
/// Every transition used to report to the log alone, so a 16 MB fetch, a
/// 180-second timeout and a permanently failed load were all indis-
/// tinguishable from a rendering bug. The resource is only marked
/// changed by a real transition — the idle poll bypasses change
/// detection — so this fires once per transition, not per frame.
pub fn surface_font_status(
    cjk: Res<CjkFonts>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    time: Res<Time>,
) {
    if !cjk.is_changed() {
        return;
    }
    if let Some((kind, text)) = status_toast(&cjk.status, cfg!(target_arch = "wasm32")) {
        toasts.push(kind, text, time.elapsed_secs_f64());
    }
}

/// Load the CJK font bytes off the main thread. Native reads the asset
/// from disk; wasm fetches it from the deploy origin (the browser cache
/// makes repeat sessions cheap), raced against a timeout because the
/// browser fetch API exposes none of its own.
fn spawn_cjk_load() -> Task<Result<Vec<u8>, String>> {
    let pool = bevy::tasks::IoTaskPool::get();
    #[cfg(not(target_arch = "wasm32"))]
    {
        pool.spawn(async move {
            std::fs::read(CJK_FONT_ASSET_PATH)
                .map_err(|e| format!("read {CJK_FONT_ASSET_PATH}: {e}"))
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        pool.spawn(async move {
            let url = cjk_font_url().ok_or_else(|| "could not resolve the font URL".to_string())?;
            let fetch = async {
                let resp = reqwest::get(&url)
                    .await
                    .map_err(|e| format!("fetch {url}: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("fetch {url}: HTTP {}", resp.status()));
                }
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| format!("read font body: {e}"))?;
                Ok(bytes.to_vec())
            };
            let timeout = async {
                gloo_timers::future::TimeoutFuture::new((CJK_FETCH_TIMEOUT_SECS * 1000) as u32)
                    .await;
                Err(format!(
                    "font fetch timed out after {CJK_FETCH_TIMEOUT_SECS}s"
                ))
            };
            futures_lite::future::or(fetch, timeout).await
        })
    }
}

/// Absolute URL of the CJK asset next to the served page — derived from
/// `window.location` so it works on any origin/path the app deploys to.
#[cfg(target_arch = "wasm32")]
fn cjk_font_url() -> Option<String> {
    let location = web_sys::window()?.location();
    let origin = location.origin().ok()?;
    let path = location.pathname().ok()?;
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Some(format!("{origin}{dir}/{CJK_FONT_ASSET_PATH}"))
}

/// Drain the finished load and swap in the CJK-extended font set.
pub fn poll_cjk_fetch(mut contexts: EguiContexts, mut cjk: ResMut<CjkFonts>) {
    // Bypass so the every-frame poll of an idle resource doesn't mark it
    // changed; a real transition below writes through normally.
    let state = cjk.bypass_change_detection();
    let Some(task) = state.task.as_mut() else {
        return;
    };
    let Some(result) = futures_lite::future::block_on(futures_lite::future::poll_once(task)) else {
        return;
    };
    cjk.task = None;
    match result {
        Ok(bytes) => {
            let Ok(ctx) = contexts.ctx_mut() else {
                // No context this frame — reinstall the finished bytes as
                // a fresh one-shot task result next frame would be more
                // machinery than the case deserves; just fail closed.
                warn!("CJK font loaded but no egui context to install into");
                cjk.status = CjkStatus::Failed;
                return;
            };
            info!("CJK font installed ({} KiB)", bytes.len() / 1024);
            ctx.set_fonts(build_font_definitions(Some(bytes)));
            cjk.status = CjkStatus::Installed;
        }
        Err(e) => {
            warn!(
                "CJK font load failed — CJK text will render as tofu this session: {e} \
                 (is {CJK_FONT_ASSET_PATH} deployed?)"
            );
            cjk.status = CjkStatus::Failed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_detection_hits_the_target_scripts() {
        for sample in [
            "你好",
            "こんにちは",
            "カタカナ",
            "안녕하세요",
            "全角！",
            "wave〜",
        ] {
            assert!(needs_cjk(sample), "{sample} should need the CJK fallback");
        }
    }

    #[test]
    fn cjk_detection_ignores_base_font_coverage() {
        for sample in [
            "hello",
            "Привет",
            "Γειά",
            "café añejo",
            "@alice.bsky.social",
            "🎉",
        ] {
            assert!(!needs_cjk(sample), "{sample} is covered by the base font");
        }
    }

    /// The chat draft is a scanned source (#1262 f359).
    ///
    /// The negative half is the one that matters: the OLD scan is
    /// `chat.messages` alone, so a test that only asserted the messages
    /// are visited would have passed against the defect. What is asserted
    /// here is that a draft nobody has sent yet reaches the detector.
    #[test]
    fn the_chat_draft_is_one_of_the_scanned_sources() {
        let mut chat = crate::state::ChatHistory {
            draft: "\u{3053}\u{3093}\u{3070}\u{3093}\u{306F}".to_owned(),
            ..Default::default()
        };
        chat.messages.push(crate::state::ChatEntry {
            did: None,
            author: "alice".to_owned(),
            text: "hello".to_owned(),
            at_epoch_secs: 0,
            delivery: crate::network::ChatDelivery::NotApplicable,
        });

        let seen: Vec<&str> = chat_sources(&chat).collect();
        assert!(
            seen.contains(&chat.draft.as_str()),
            "the draft must be scanned before it is sent"
        );
        assert!(seen.contains(&"hello") && seen.contains(&"alice"));
        assert!(
            chat_sources(&chat).any(needs_cjk),
            "an unsent Japanese draft must be enough to trigger the load"
        );

        // The control: without the draft this history is pure ASCII, which
        // is exactly why the old scan never fired on it.
        chat.draft.clear();
        assert!(!chat_sources(&chat).any(needs_cjk));
    }

    /// The gap table names the script it found, and stays quiet about
    /// everything the app can actually draw (#1262 f360).
    #[test]
    fn unsupported_scripts_are_named_and_only_when_real() {
        assert_eq!(
            unsupported_script("\u{05E9}\u{05DC}\u{05D5}\u{05DD}"),
            Some("Hebrew")
        );
        assert_eq!(
            unsupported_script("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"),
            Some("Arabic")
        );
        assert_eq!(
            unsupported_script("\u{0E2A}\u{0E27}\u{0E31}\u{0E2A}\u{0E14}\u{0E35}"),
            Some("Thai")
        );
        assert_eq!(
            unsupported_script("\u{0928}\u{092E}\u{0938}\u{094D}\u{0924}\u{0947}"),
            Some("Devanagari")
        );
        // A Latin sentence with one Arabic word still reports the gap.
        assert_eq!(
            unsupported_script("hi \u{0639}\u{0644}\u{064A}"),
            Some("Arabic")
        );

        for covered in [
            "hello",
            "\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}",
            "\u{0393}\u{03B5}\u{03B9}\u{03AC}",
            "caf\u{00E9}",
            "@alice.bsky.social",
        ] {
            assert_eq!(unsupported_script(covered), None, "{covered} is covered");
        }
        // CJK is a gap the app CAN close, so it is not one of these — it
        // has a fetch, and reporting it would tell the user to give up on
        // text that is about to render.
        assert_eq!(unsupported_script("\u{4F60}\u{597D}"), None);
        assert!(needs_cjk("\u{4F60}\u{597D}"));
    }

    /// The font lifecycle says something on exactly the transitions a user
    /// can be hurt by, and stays quiet on the rest (#1262 f361).
    ///
    /// `slow_load` is a parameter precisely so both answers are reachable
    /// from a native test run — the wasm branch is otherwise unreachable
    /// by every gate this repo has.
    #[test]
    fn the_font_lifecycle_speaks_only_when_it_has_something_to_say() {
        use crate::ui::toast::ToastKind;

        // A 16 MB download over an unknown link: worth announcing.
        assert_eq!(
            status_toast(&CjkStatus::Fetching, true).map(|(k, _)| k),
            Some(ToastKind::Info)
        );
        // A local file read that finishes before the next frame is not:
        // a six-second toast about a load that already completed is a
        // worse lie than silence.
        assert_eq!(status_toast(&CjkStatus::Fetching, false), None);

        // Terminal, and the only state the user can do nothing about —
        // this is the one that used to be a `warn!` nobody reads.
        assert_eq!(
            status_toast(&CjkStatus::Failed, false).map(|(k, _)| k),
            Some(ToastKind::Warn)
        );
        assert_eq!(
            status_toast(&CjkStatus::Failed, true).map(|(k, _)| k),
            Some(ToastKind::Warn)
        );

        // Nothing has happened, or it worked: the text appearing IS the
        // message.
        assert!(status_toast(&CjkStatus::Dormant, true).is_none());
        assert!(status_toast(&CjkStatus::Installed, true).is_none());
    }

    /// The drawn-text channel carries a sighting to the detector and is
    /// emptied by the read (#1262 f359).
    ///
    /// The clear is the half worth testing: a sighting left in egui's
    /// memory would be re-reported on every later frame, which for the
    /// script-gap toast means the message returns forever.
    #[test]
    fn the_drawn_text_channel_reports_once_and_clears() {
        let ctx = egui::Context::default();

        assert!(!take_drawn_sighting(&ctx).cjk, "nothing drawn yet");

        note_drawn_text(&ctx, "ordinary latin text");
        let quiet = take_drawn_sighting(&ctx);
        assert!(
            !quiet.cjk && quiet.gap.is_none(),
            "covered text is not news"
        );

        note_drawn_text(&ctx, "\u{540D}\u{524D}");
        note_drawn_text(&ctx, "\u{05E9}\u{05DC}\u{05D5}\u{05DD}");
        let seen = take_drawn_sighting(&ctx);
        assert!(seen.cjk, "sightings accumulate across draw sites");
        assert_eq!(seen.gap, Some("Hebrew"));

        let after = take_drawn_sighting(&ctx);
        assert!(
            !after.cjk && after.gap.is_none(),
            "a drained sighting must not be reported again next frame"
        );
    }

    /// The font set builder is the whole contract: Noto leads the
    /// proportional family, egui's fonts stay as tail, monospace keeps
    /// its primary, and the CJK face lands at the very end of both
    /// families when provided.
    #[test]
    fn font_definitions_order_base_then_fallbacks() {
        let defs = build_font_definitions(None);
        let prop = &defs.families[&egui::FontFamily::Proportional];
        assert_eq!(prop.first().map(String::as_str), Some("noto-sans"));
        assert!(prop.len() > 1, "egui's embedded fonts must remain as tail");
        let mono = &defs.families[&egui::FontFamily::Monospace];
        assert_ne!(mono.first().map(String::as_str), Some("noto-sans"));
        assert!(mono.iter().any(|f| f == "noto-sans"));

        let with_cjk = build_font_definitions(Some(vec![0u8; 4]));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            assert_eq!(
                with_cjk.families[&family].last().map(String::as_str),
                Some("noto-cjk"),
                "CJK must be the last fallback in {family:?}"
            );
        }
    }
}

/// The crate's source scans over its own UI, and the helpers they share.
///
/// Four laws live here — glyphs the bundled fonts cannot draw, the hosted
/// editor's glyph list, numeric widgets built without the locale parser,
/// and US spellings in copy — because they ask the same question of the
/// same files and a second copy of the walk or the literal lexer is how
/// two scans drift apart. `pub(crate)` so a scan that has to live
/// elsewhere can still borrow the helpers rather than re-deriving them:
/// `room::widgets`' sRGB-colour-widget scan does exactly that.
#[cfg(test)]
pub(crate) mod glyph_coverage_tests {
    use super::*;

    /// Files outside `src/ui` whose string literals are rendered verbatim
    /// as UI labels (#1223).
    ///
    /// `network::presence` derives the People roster's status chip and its
    /// hover sentence, so its literals are drawn by `people_ui` and are
    /// exactly as capable of shipping tofu as anything under `src/ui` — the
    /// `⋯` this list was added for is one the bundled fonts cannot draw.
    /// Add a path here when a module starts producing text a UI surface
    /// prints without touching it.
    const EXTRA_LABEL_SOURCES: &[&str] = &[
        "src/network/presence.rs",
        // #1240: `RecoveryReason::toast` and the return-to-spawn refusals
        // are drawn verbatim by the toast stack.
        "src/player/respawn.rs",
        // #1246: `AssetFetchError::sentence` and `AssetFailure::status_line`
        // are printed verbatim by every asset field in the room editor, by
        // the loading screen's ambient row and by the arrival toast.
        "src/world_builder/asset_failure.rs",
        // #1267: `GeneratorKind::display_name` / `blurb` are the creation
        // menus' entries and the gift modal's kind line — the labels the
        // CamelCase serde tags used to be.
        "src/pds/generator.rs",
        // #1267: `socket_label` names every wear surface's socket, and
        // `attachment_label` names a prop in a preflight refusal.
        "src/pds/avatar/wardrobe.rs",
    ];

    /// Non-ASCII glyphs drawn by the sculpting sections the Body tab HOSTS
    /// from `bevy_symbios_avatar::editor` (#1257 f116).
    ///
    /// The walk above is rooted at `src/ui` plus [`EXTRA_LABEL_SOURCES`],
    /// and both are paths under this crate — so the one surface the avatar
    /// epic moved a whole editor into was the one surface no gate could see.
    /// The two most-used controls in the Body tab are a pair of these
    /// arrows, and a silently-tofu arrow is unfindable in review because it
    /// looks like a styled button until you render it.
    ///
    /// A hand-kept list rather than a walk of the dependency's source: that
    /// source lives in the cargo registry, at a path that depends on the
    /// resolved version and on `CARGO_HOME`, which is not something a test
    /// can rely on in CI or a vendored build. **Refresh this on a
    /// `bevy_symbios_avatar` bump** — it is named in the dependency-bump
    /// checklist for exactly that reason. Being stale costs coverage, never
    /// a false failure; the list is a floor, not a claim of completeness.
    const HOSTED_EDITOR_GLYPHS: &[char] = &[
        '·', // U+00B7, axis readouts
        '—', // U+2014, section dashes
        '•', // U+2022, list bullets
        '…', // U+2026, truncation
        '▶', // U+25B6, seed-hunt step forward
        '◀', // U+25C0, seed-hunt step back
        '⚠', // U+26A0, the generator-mismatch warning
    ];

    /// The charmaps of every face the proportional family falls back
    /// through, in the order the app installs them (Noto Sans first, egui's
    /// embedded tail after, no CJK).
    struct BaseAtlas {
        faces: Vec<std::sync::Arc<egui::FontData>>,
    }

    impl BaseAtlas {
        fn new() -> Self {
            let defs = build_font_definitions(None);
            let faces = defs.families[&egui::FontFamily::Proportional]
                .iter()
                .map(|name| defs.font_data[name].clone())
                .collect();
            Self { faces }
        }

        /// Whether some face in the chain owns a glyph for `c` — the exact
        /// question epaint's face resolution asks per character.
        ///
        /// Deliberately NOT `Fonts::has_glyph`: in epaint 0.35 that compares
        /// the *face* a char resolves to against the face that owns `�`, so
        /// every glyph Noto Sans (our primary, which has U+FFFD) carries —
        /// arrows, ⚠, ✔ — reports as missing. Nor a laid-out galley: its
        /// atlas rects are not a tofu signature. The charmap is.
        fn draws(&self, c: char) -> bool {
            use skrifa::MetadataProvider;
            self.faces.iter().any(|face| {
                let font = skrifa::FontRef::from_index(&face.font, face.index)
                    .expect("a bundled face parses");
                font.charmap().map(c).is_some()
            })
        }
    }

    /// The contents of every `"…"` string literal in `source`.
    ///
    /// A deliberately small lexer, but a whole-source one: it used to run
    /// per line, splitting each on `//` first, which had two consequences
    /// that cost real coverage (#1266).
    ///
    /// **A backslash-continued literal was invisible past its first
    /// line.** Almost every sentence in this UI is written that way — a
    /// confirm body, a hover, a banner — so a scan for a word in prose saw
    /// only the opening fragment. Twelve of the fifteen "PDS" strings the
    /// vocabulary sweep had to find lived on continuation lines.
    ///
    /// **`"https://…"` lexed as a string plus a comment.** Splitting on
    /// `//` before knowing whether you are inside a literal cuts URLs in
    /// half.
    ///
    /// So the scan is a real (if tiny) lexer: `//` ends a line only
    /// OUTSIDE a literal, a literal runs to its closing quote across
    /// newlines, and a char literal is recognised so that `'"'` cannot
    /// open a string that swallows the rest of the file. Escapes stay
    /// opaque — only the raw glyphs matter — which means a continued
    /// literal comes back carrying the source's own indentation. That is
    /// fine for every needle these scans look for and would not be for a
    /// whitespace check; nothing here does one.
    ///
    /// A mis-lexed literal still costs coverage, never a false failure.
    fn string_literals(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '/' if chars.peek() == Some(&'/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                // A char literal, but only when it really is one: `'a'` and
                // `'\n'` are, `'a` opening a lifetime is not, and treating a
                // lifetime as a literal would desync everything after it.
                '\'' if is_char_literal(&chars) => {
                    if chars.peek() == Some(&'\\') {
                        chars.next();
                    }
                    chars.next();
                    chars.next();
                }
                '"' => {
                    let mut literal = String::new();
                    loop {
                        match chars.next() {
                            None | Some('"') => break,
                            Some('\\') => {
                                chars.next();
                            }
                            Some(other) => literal.push(other),
                        }
                    }
                    out.push(literal);
                }
                _ => {}
            }
        }
        out
    }

    /// Whether the `'` just consumed opens a char literal rather than a
    /// lifetime. `rest` starts at the character after the quote.
    fn is_char_literal(rest: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
        let mut probe = rest.clone();
        match probe.next() {
            Some('\\') => {
                probe.next();
                probe.next() == Some('\'')
            }
            Some(_) => probe.next() == Some('\''),
            None => false,
        }
    }

    /// `source` with every `bevy::log` macro invocation blanked out.
    ///
    /// A log line is not UI copy — nobody reads `info!("Room record saved
    /// to PDS")` on a screen — and the logs are by far the largest
    /// population of strings in this tree that legitimately speak the
    /// wire's vocabulary. Without this cut the vocabulary scans would
    /// either fail on the logs or need a per-line exception list, and an
    /// exception list is how a scan stops meaning anything.
    ///
    /// Lines are blanked rather than removed so reported line numbers
    /// still point at the source. Parentheses are balanced from the
    /// macro's opening line, so a multi-line `warn!(\n "…",\n x\n);` goes
    /// whole. A macro whose parens never balance would swallow the rest of
    /// the file: that costs coverage, never a false failure, which is the
    /// trade every helper here makes.
    pub(crate) fn without_log_macros(source: &str) -> String {
        const MACROS: &[&str] = &["info!(", "warn!(", "error!(", "debug!(", "trace!("];
        let mut out = String::with_capacity(source.len());
        let mut depth: i32 = 0;
        for line in source.lines() {
            let code = line.split("//").next().unwrap_or("");
            if depth == 0 {
                match MACROS.iter().filter_map(|m| code.find(m)).min() {
                    Some(at) => depth = paren_balance(&code[at..]),
                    None => {
                        out.push_str(line);
                        out.push('\n');
                        continue;
                    }
                }
            } else {
                depth += paren_balance(code);
            }
            depth = depth.max(0);
            out.push('\n');
        }
        out
    }

    /// Open parentheses minus closing ones. Quotes are not tracked: a `(`
    /// inside a log message inflates the count and swallows a line or two
    /// more than it should, which costs coverage rather than causing a
    /// false failure.
    fn paren_balance(code: &str) -> i32 {
        code.matches('(').count() as i32 - code.matches(')').count() as i32
    }

    /// Every non-ASCII glyph a UI label can show must exist in the base
    /// font set (#1105): the Attachments tab's "◈ Drag in world" shipped a
    /// code point neither Noto Sans nor egui's embedded faces carry, and
    /// it rendered as tofu in-world — nothing at build time can see a
    /// missing glyph, so this walks `src/ui/**` and asks the real font
    /// atlas. CJK is exempt because it is the lazily-loaded fallback's
    /// job ([`needs_cjk`]).
    ///
    /// [`EXTRA_LABEL_SOURCES`] extends the walk to files that produce UI
    /// label text from OUTSIDE `src/ui` (#1223). The whole of `src/` cannot
    /// be walked instead — it is full of log lines and wire strings nobody
    /// renders — so a module that hands `src/ui` a string to draw verbatim
    /// has to name itself here.
    /// Every `.rs` file under `rel`, recursively.
    fn rust_sources_under(rel: &str) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{rel} readable: {e}")) {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        out
    }

    /// `source` up to its first `#[cfg(test)]`, which is where every file
    /// in this crate puts its tests.
    ///
    /// Needed because a scan that bans a string is itself a file
    /// containing that string — both scans below found their own needles
    /// before this existed. Test code draws no widgets and ships no copy,
    /// so cutting it is not a compromise. An item-level `#[cfg(test)]`
    /// earlier in a file would truncate the scan early: that costs
    /// coverage, never a false pass, which is the same trade the literal
    /// lexer makes.
    pub(crate) fn non_test_source(source: &str) -> &str {
        // The cut is at the `#[cfg(test)]` that introduces the test
        // MODULE, not at the first one in the file.
        //
        // It used to be the first one, on the reasoning that an attribute
        // at column 0 is the crate's convention for a top-level test
        // module. That stopped being true, and silently: `toolbar.rs`
        // carries a `#[cfg(test)] const` at line 250 of 1900, and
        // `room/placements.rs` and `room/generators/tree.rs` each carry a
        // `#[cfg(test)] thread_local!` counter — so for those three files
        // every scan built on this helper (the four vocabulary scans, the
        // glyph law, the raw-number-widget ban, the panel-flag guard) had
        // been reading the first few hundred lines and calling it the
        // file. A blind scan passes, which is the failure mode none of
        // them can report.
        //
        // Found by `every_panel_flag_write_is_guarded`'s floor assertion —
        // the guard count fell by one when a fourth such item was added.
        // That floor is the only reason this was visible at all.
        //
        // The `starts_with` arm is for a source that is nothing but tests:
        // no file in the tree looks like that, but a caller's synthetic
        // control does, and a helper that answers wrongly on the simplest
        // input is a helper nobody can write a control for.
        if source.starts_with("#[cfg(test)]") && opens_a_module(source, "#[cfg(test)]".len()) {
            return "";
        }
        let mut from = 0;
        while let Some(at) = source[from..].find("\n#[cfg(test)]") {
            let at = from + at;
            let after = at + "\n#[cfg(test)]".len();
            if opens_a_module(source, after) {
                return &source[..at];
            }
            from = after;
        }
        source
    }

    /// Whether the line after `at` declares a module — the shape that
    /// makes a `#[cfg(test)]` the file's test module rather than one
    /// test-only item among the production code.
    fn opens_a_module(source: &str, at: usize) -> bool {
        source[at..].lines().nth(1).is_some_and(|line| {
            let line = line.trim_start();
            line.starts_with("mod ")
                || line.starts_with("pub mod ")
                || line.starts_with("pub(crate) mod ")
                || line.starts_with("pub(super) mod ")
        })
    }

    /// Whether `line` constructs a numeric widget the raw way, ignoring
    /// anything after a `//`.
    fn builds_a_raw_numeric_widget(line: &str) -> bool {
        let code = line.split("//").next().unwrap_or("");
        code.contains("egui::DragValue::new(") || code.contains("egui::Slider::new(")
    }

    /// The US spelling this literal drifted into, if it is UI copy at all.
    ///
    /// Identifier-shaped literals are not copy: `"color_edit_button_rgb"`
    /// is an egui method name quoted inside another source scan, and
    /// nobody reads it on screen.
    fn us_spelling(literal: &str) -> Option<&'static str> {
        let identifier_shaped = !literal.is_empty()
            && literal
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if identifier_shaped {
            return None;
        }
        ["color", "Color", "center", "Center"]
            .into_iter()
            .find(|wrong| literal.contains(wrong))
    }

    /// A repo-relative path, for a failure message a reader can act on.
    fn short(path: &std::path::Path) -> String {
        path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(path)
            .display()
            .to_string()
    }

    #[test]
    fn every_ui_label_glyph_is_in_the_base_font_set() {
        let mut sources: Vec<std::path::PathBuf> = EXTRA_LABEL_SOURCES
            .iter()
            .map(|rel| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .collect();
        sources.extend(rust_sources_under("src/ui"));
        assert!(
            sources.len() > EXTRA_LABEL_SOURCES.len(),
            "the walk found no UI sources"
        );

        let atlas = BaseAtlas::new();
        let mut missing = Vec::new();
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("UI source is readable");
            for literal in string_literals(&source) {
                for c in literal.chars() {
                    if c.is_ascii() || needs_cjk(&c.to_string()) {
                        continue;
                    }
                    if !atlas.draws(c) {
                        missing.push(format!(
                            "{} U+{:04X} in {}",
                            c,
                            u32::from(c),
                            path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                                .unwrap_or(&path)
                                .display()
                        ));
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "UI label glyphs the bundled fonts cannot draw (tofu in-world):\n  {}",
            missing.join("\n  ")
        );
    }

    /// Every glyph the HOSTED sculpting sections draw must be in the base
    /// font set too (#1257 f116).
    ///
    /// Same law as [`every_ui_label_glyph_is_in_the_base_font_set`], asked
    /// of the one editor this crate draws but does not own. It has already
    /// bitten twice inside `src/ui` — #861 for ✓/● and #1105 for ◈ — and
    /// nothing at build time can see a missing glyph.
    #[test]
    fn every_hosted_editor_glyph_is_in_the_base_font_set() {
        let atlas = BaseAtlas::new();
        let missing: Vec<String> = HOSTED_EDITOR_GLYPHS
            .iter()
            .filter(|c| !atlas.draws(**c))
            .map(|c| format!("{c} U+{:04X}", u32::from(*c)))
            .collect();
        assert!(
            missing.is_empty(),
            "glyphs the hosted avatar editor draws that the bundled fonts cannot \
             (tofu on the Body tab):\n  {}",
            missing.join("\n  ")
        );
        // The list is only worth anything if it is actually being probed —
        // an empty one would pass vacuously for the rest of time.
        assert!(HOSTED_EDITOR_GLYPHS.len() >= 6);
        assert!(
            HOSTED_EDITOR_GLYPHS.contains(&'◀') && HOSTED_EDITOR_GLYPHS.contains(&'▶'),
            "the seed-hunt arrows are the two most-used controls on the tab"
        );
    }

    /// The check itself must be able to tell a drawn glyph from tofu:
    /// plain Latin and a Noto Sans symbol draw, a private-use code point
    /// no face carries does not. Without this the coverage walk could
    /// pass by comparing nothing.
    #[test]
    fn the_atlas_probe_separates_drawn_glyphs_from_tofu() {
        let atlas = BaseAtlas::new();
        assert!(atlas.draws('a'));
        assert!(atlas.draws('Ж'), "Noto Sans carries Cyrillic");
        assert!(
            atlas.draws('✔'),
            "the emoji tail carries the #861 checkmark"
        );
        assert!(!atlas.draws('✓'), "U+2713 is the #861 tofu");
        assert!(!atlas.draws('\u{E000}'), "a private-use code point is tofu");
        assert!(
            !atlas.draws('◈'),
            "U+25C8 is the #1105 tofu; if a font now carries it, drop this line"
        );
        // Plain arrows are not emoji, so the emoji faces skip them and Noto
        // Sans has none; the toolbar's key hints were tofu until #1105.
        assert!(!atlas.draws('←'));
    }

    /// Every script [`UNSUPPORTED_SCRIPTS`] names must really be a gap
    /// (#1262 f360).
    ///
    /// The table drives a message telling the user their script cannot be
    /// displayed. If a font change ever closes one of those gaps, the
    /// message becomes a lie about text that is rendering perfectly well —
    /// so the row is probed against the same charmaps epaint resolves
    /// through, and a closed gap fails here rather than shipping.
    #[test]
    fn the_unsupported_script_table_names_real_gaps() {
        let atlas = BaseAtlas::new();
        let drawn: Vec<String> = UNSUPPORTED_SCRIPTS
            .iter()
            .filter(|gap| atlas.draws(gap.sample))
            .map(|gap| format!("{} (U+{:04X})", gap.name, u32::from(gap.sample)))
            .collect();
        assert!(
            drawn.is_empty(),
            "these scripts are no longer gaps — drop their rows, the toast \
             would be telling users text they can see cannot be shown:\n  {}",
            drawn.join("\n  ")
        );
    }

    /// Numeric entry goes through the locale-aware constructors, always
    /// (#1264 f364).
    ///
    /// egui offers no `Style`-level parser hook, so a decimal comma has to
    /// be handled per widget, which means per construction site — and
    /// there are 78 of them across 16 files. A helper nobody is obliged to
    /// call fixes this once and loses it again the next time somebody
    /// reaches for `egui::DragValue::new`, which is exactly how the defect
    /// got this wide. `ui::num` is the only file allowed to say it.
    #[test]
    fn the_only_numeric_widgets_are_the_locale_aware_ones() {
        let mut sources = rust_sources_under("src/ui");
        // The gizmo's transform fields are numeric entry too, and they
        // live outside `src/ui`.
        sources.extend(rust_sources_under("src/editor_gizmo"));

        assert!(sources.len() > 20, "the walk found no sources to scan");

        // The control. Both scans in this module found their own needles
        // until test source was excluded, and the fix could just as easily
        // have blinded them entirely — a scan that cannot see the thing it
        // bans passes forever and proves nothing.
        assert!(builds_a_raw_numeric_widget(
            "  ui.add(egui::DragValue::new(&mut v));"
        ));
        assert!(builds_a_raw_numeric_widget(
            "egui::Slider::new(&mut v, 0.0..=1.0)"
        ));
        assert!(!builds_a_raw_numeric_widget(
            "  ui.add(crate::ui::num::drag(&mut v));"
        ));
        assert!(
            !builds_a_raw_numeric_widget("  // egui::DragValue::new is banned here"),
            "a mention in a comment is not a call site"
        );

        let mut raw = Vec::new();
        for path in sources {
            if path.ends_with("num.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source is readable");
            for (n, line) in non_test_source(&source).lines().enumerate() {
                if builds_a_raw_numeric_widget(line) {
                    raw.push(format!("{}:{}", short(&path), n + 1));
                }
            }
        }
        assert!(
            raw.is_empty(),
            "numeric widgets built without the locale parser — use \
             `crate::ui::num::drag` / `::slider`, which accept a decimal comma:\n  {}",
            raw.join("\n  ")
        );
    }

    /// Every raw `egui::TextEdit::{singleline,multiline}` construction in
    /// `source` that is NOT an argument to `affordances::text_edit`, as
    /// `(line, snippet)` pairs.
    ///
    /// A window over the preceding source rather than a per-line test,
    /// because `cargo fmt` puts the constructor on its own line the
    /// moment the call wraps — so the call and the construction are
    /// routinely two lines apart, and a per-line rule flags every correct
    /// site.
    fn raw_text_fields(source: &str) -> Vec<(usize, String)> {
        const LOOKBEHIND: usize = 200;
        let mut out = Vec::new();
        for needle in ["egui::TextEdit::singleline(", "egui::TextEdit::multiline("] {
            let mut from = 0;
            while let Some(at) = source[from..].find(needle) {
                let at = from + at;
                from = at + needle.len();
                let line_start = source[..at].rfind('\n').map_or(0, |n| n + 1);
                // A mention inside a `//` comment is not a call site.
                if source[line_start..at].contains("//") {
                    continue;
                }
                let window = &source[at.saturating_sub(LOOKBEHIND)..at];
                if window.contains("text_edit(") || window.contains("text_edit_enabled(") {
                    continue;
                }
                let line = source[..at].matches('\n').count() + 1;
                out.push((line, source[at..from].to_string()));
            }
        }
        out.sort_unstable();
        out
    }

    /// Every text field goes through `affordances::text_edit` (#1284).
    ///
    /// egui 0.35 paints a FOCUSED field's frame with
    /// `visuals.selection.stroke`, which in this app is `selection_text` —
    /// the near-black label colour of a selected chip. Since #1283 gave a
    /// resting field a gray-105 edge, focusing one *removed* its border.
    /// The helper scopes a proper ring to the widget; a field added
    /// directly with `ui.add` silently opts out of it, and there is
    /// nothing on screen to notice, because the defect is the ABSENCE of a
    /// line.
    ///
    /// Same shape as `the_only_numeric_widgets_are_the_locale_aware_ones`
    /// and for the same reason (#1264 f364): a helper nobody is obliged to
    /// call fixes the problem once and loses it at the next call site.
    #[test]
    fn the_only_text_fields_are_the_ring_aware_ones() {
        let mut sources = rust_sources_under("src/ui");
        sources.extend(rust_sources_under("src/editor_gizmo"));
        assert!(sources.len() > 20, "the walk found no sources to scan");

        // The controls, both ways round — a scan that cannot see what it
        // bans passes forever.
        assert_eq!(
            raw_text_fields("ui.add(egui::TextEdit::singleline(&mut s));").len(),
            1
        );
        assert_eq!(
            raw_text_fields("ui.add(egui::TextEdit::multiline(&mut s));").len(),
            1
        );
        assert!(
            raw_text_fields(
                "affordances::text_edit(\n    ui,\n    egui::TextEdit::singleline(&mut s),\n)"
            )
            .is_empty(),
            "the wrapped form is the correct one, and fmt puts it on its own line"
        );
        assert!(
            raw_text_fields("  // egui::TextEdit::singleline is banned here").is_empty(),
            "a mention in a comment is not a call site"
        );

        let mut raw = Vec::new();
        for path in sources {
            if path.ends_with("affordances.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source is readable");
            for (line, _) in raw_text_fields(non_test_source(&source)) {
                raw.push(format!("{}:{line}", short(&path)));
            }
        }
        assert!(
            raw.is_empty(),
            "text fields built without the focus ring — use \
             `crate::ui::affordances::text_edit` / `::text_edit_enabled`, which paint \
             a focused field's edge in the accent instead of erasing it:\n  {}",
            raw.join("\n  ")
        );
    }

    // -- The guarded-dirty scan (#1270 f121) ------------------------------

    /// A `let mut <local> = panels.<field>;` binding, as
    /// `(line, local, field)`.
    fn panel_flag_bindings(source: &str) -> Vec<(usize, String, String)> {
        let mut out = Vec::new();
        for (n, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let Some(rest) = code.trim_start().strip_prefix("let mut ") else {
                continue;
            };
            let Some((local, tail)) = rest.split_once(" = panels.") else {
                continue;
            };
            let field = ident_at(tail);
            if field.is_empty() || !tail[field.len()..].starts_with(';') {
                continue;
            }
            out.push((n + 1, local.trim().to_string(), field));
        }
        out
    }

    /// Every `panels.<field> = <rhs>;` assignment, as `(line, field, rhs)`.
    ///
    /// A `==` comparison is not a write, and neither is the `panels.x` on
    /// the right of a `let` — both are excluded by requiring a single `=`
    /// immediately after the field name.
    fn panel_flag_writes(source: &str) -> Vec<(usize, String, String)> {
        let mut out = Vec::new();
        for (n, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let mut from = 0;
            while let Some(at) = code[from..].find("panels.") {
                let at = from + at;
                from = at + "panels.".len();
                // `ui_panels.` is a different binding, not this one.
                if code[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    continue;
                }
                let field = ident_at(&code[from..]);
                if field.is_empty() {
                    continue;
                }
                let tail = code[from + field.len()..].trim_start();
                let Some(rhs) = tail.strip_prefix('=') else {
                    continue;
                };
                if rhs.starts_with('=') {
                    continue;
                }
                let rhs = rhs.trim().trim_end_matches([';', ',']).trim();
                out.push((n + 1, field, rhs.to_string()));
            }
        }
        out
    }

    /// Lines that hand a `UiPanels` field straight to a widget as `&mut`.
    fn direct_panel_opens(source: &str) -> Vec<usize> {
        source
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                line.split("//")
                    .next()
                    .unwrap_or("")
                    .contains("&mut panels.")
            })
            .map(|(n, _)| n + 1)
            .collect()
    }

    /// The leading Rust identifier of `s`, or `""`.
    fn ident_at(s: &str) -> String {
        s.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect()
    }

    /// Every window's open flag is written on the CLOSING EDGE, never per
    /// frame (#1270 f121, the #879 guarded-dirty rule).
    ///
    /// `UiPanels` is a `Resource`, so any `ResMut::deref_mut` stamps its
    /// change tick — and `prefs::save_prefs_when_changed` ORs
    /// `panels.is_changed()` into a 1.0 s trailing debounce. A resource
    /// that is never quiet therefore produces a full prefs
    /// serialise-and-write about once a second, forever: `std::fs::write`
    /// on native, a synchronous `localStorage.setItem` of the whole blob
    /// on wasm, and it starves the debounce for every OTHER pref too.
    ///
    /// The Catalogue was the one window that wrote `panels.catalogue =
    /// open;` unconditionally, and one line was enough to defeat the whole
    /// programme app-wide. Eight sibling windows carried a `#879` comment
    /// explaining the idiom and nothing enforced it, which is how the ninth
    /// got written. Three rules, all cheap:
    ///
    /// **No `&mut panels.…`.** `egui::Window::open` takes `&mut bool`;
    /// pointed at the resource directly it dirties every frame the window
    /// draws.
    ///
    /// **A write's right-hand side is a bool literal.** Every legitimate
    /// write in the tree opens or closes a window at a known moment
    /// (`shortcuts.rs`' Esc arms, `editable.rs`' "take me there" buttons).
    /// `panels.x = some_local` is the defect shape: a value that came out
    /// of the resource going straight back into it, every frame.
    ///
    /// **A binding implies its guard.** A file that takes `let mut open =
    /// panels.x;` must also contain `if panels.x && !open`, the closing
    /// edge. This is the positive half: rule two alone is satisfied by a
    /// window that reads the flag and never writes it back at all, which
    /// would leave the close button inert.
    ///
    /// What this does NOT catch is a bool-literal write placed on a path
    /// that runs every frame. Nothing in the tree looks like that and no
    /// syntactic rule could tell it from `shortcuts.rs`' keypress arms; the
    /// change-tick pairing in `ui::perf` is what measures the behaviour
    /// itself.
    #[test]
    fn every_panel_flag_write_is_guarded() {
        let sources = rust_sources_under("src/ui");
        assert!(sources.len() > 20, "the walk found no sources to scan");

        // Controls, both ways round: the shape that shipped and the shape
        // that replaced it. A scan that cannot see what it bans passes
        // forever.
        let shipped = "    let mut open = panels.catalogue;\n    panels.catalogue = open;\n";
        assert_eq!(
            panel_flag_bindings(shipped),
            vec![(1, "open".to_string(), "catalogue".to_string())]
        );
        assert_eq!(
            panel_flag_writes(shipped),
            vec![(2, "catalogue".to_string(), "open".to_string())],
            "the unconditional write-back is what f121 was"
        );
        let guarded = "    let mut open = panels.catalogue;\n    if panels.catalogue && !open {\n        panels.catalogue = false;\n    }\n";
        assert_eq!(
            panel_flag_writes(guarded)
                .iter()
                .map(|(_, _, rhs)| rhs.as_str())
                .collect::<Vec<_>>(),
            vec!["false"],
            "the guarded form writes a literal on the edge"
        );
        assert_eq!(panel_flag_writes("if panels.chat == open {").len(), 0);
        assert_eq!(direct_panel_opens(".open(&mut panels.chat)").len(), 1);
        assert_eq!(
            direct_panel_opens("// `.open(&mut panels.chat)` would dirty it").len(),
            0,
            "a mention in a comment is not a call site"
        );

        let mut faults = Vec::new();
        let mut guards_seen = 0usize;
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("source is readable");
            let code = non_test_source(&source);
            for line in direct_panel_opens(code) {
                faults.push(format!(
                    "{}:{line}: `&mut panels.…` hands the resource to a widget; take a \
                     local copy and write back on the closing edge",
                    short(&path)
                ));
            }
            for (line, field, rhs) in panel_flag_writes(code) {
                if rhs != "true" && rhs != "false" {
                    faults.push(format!(
                        "{}:{line}: `panels.{field} = {rhs};` writes a non-literal — if \
                         that is the window's own open flag it runs every frame",
                        short(&path)
                    ));
                }
            }
            for (line, local, field) in panel_flag_bindings(code) {
                let guard = format!("if panels.{field} && !{local}");
                if code.contains(&guard) {
                    guards_seen += 1;
                } else {
                    faults.push(format!(
                        "{}:{line}: binds `panels.{field}` into `{local}` but never closes \
                         the window — expected `{guard} {{ panels.{field} = false; }}`",
                        short(&path)
                    ));
                }
            }
        }
        assert!(
            faults.is_empty(),
            "UiPanels writes that dirty the resource per frame and starve the prefs \
             save debounce (#879, #1270 f121):\n  {}",
            faults.join("\n  ")
        );
        assert!(
            guards_seen >= 8,
            "only {guards_seen} guarded windows found — the binding scan has gone blind"
        );
    }

    /// The `ResMut` system parameters declared in `source` that are still
    /// change-detecting where the widgets are drawn.
    ///
    /// A param shadowed by `let x = x.bypass_change_detection();` is dropped:
    /// that IS the fix for a resource with no change-tick consumer, and it is
    /// the idiom the toolbar has used since #879. Dropping it here is what
    /// lets the fix be one line at the top of a system rather than a rename
    /// of every use.
    fn res_mut_params(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in source.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix("mut ") else {
                continue;
            };
            let Some((name, ty)) = rest.split_once(": ") else {
                continue;
            };
            if ty.starts_with("ResMut<") && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                out.push(name.to_string());
            }
        }
        out.sort();
        out.dedup();
        out.retain(|name| {
            !source.contains(&format!("let {name} = {name}.bypass_change_detection()"))
        });
        out
    }

    /// Lines in `source` that hand an egui widget a `&mut` straight through
    /// one of `params`, as `(line number, the parameter)`.
    fn resource_fields_handed_to_widgets(source: &str, params: &[String]) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for (n, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            // A widget CALL, in any of the three shapes this tree writes:
            // a `ui.` method, an `egui::` constructor (`TextEdit::singleline`
            // takes its `&mut` at construction, which is where two of the
            // four live sites were), and `Window::open`.
            if !code.contains("ui.") && !code.contains("egui::") && !code.contains(".open(") {
                continue;
            }
            for param in params {
                if code.contains(&format!("&mut {param}.")) {
                    out.push((n + 1, param.clone()));
                }
            }
        }
        out
    }

    /// No egui widget is handed a `&mut` straight through a `ResMut`
    /// (#1274 f177) — the general form of the rule `panels.*` already has.
    ///
    /// Bevy's `ResMut::deref_mut` stamps the change tick on ACCESS and never
    /// compares, so `ui.checkbox(&mut wireframe.global, ..)` marks
    /// `WireframeConfig` changed on every frame the tab is drawn, and Bevy
    /// re-runs `wireframe_config_changed` — and re-uploads the global
    /// material — on each of them. The file it shipped in documents the
    /// idiom for `UiPanels` seventy lines further up.
    ///
    /// **This is deliberately the general rule rather than a second
    /// panel-shaped one.** `every_panel_flag_write_is_guarded` knows what a
    /// panel flag MEANS (a window opening and closing, so its writes are
    /// bool literals on known edges) and could not be widened without
    /// losing that. What generalises is the hazard itself — a `&mut`
    /// reaching a widget through a `ResMut` — and it is one line to state
    /// over the resources a file actually declares. A rule that only knew
    /// about `panels` is how the ninth window got written.
    ///
    /// The fix is always the same shape: copy the field into a local, hand
    /// the widget `&mut local`, and write back through the `ResMut` only
    /// when it differs.
    #[test]
    fn no_widget_writes_straight_through_a_resmut() {
        // Controls. The declaration form, and both the shape that shipped
        // and the shape that replaced it.
        let shipped = "    mut wireframe: ResMut<WireframeConfig>,\n\
                       fn f(ui: &mut Ui) {\n\
                       ui.checkbox(&mut wireframe.global, \"Wireframe mode\");\n}";
        assert_eq!(res_mut_params(shipped), vec!["wireframe".to_string()]);
        assert_eq!(
            resource_fields_handed_to_widgets(shipped, &res_mut_params(shipped)),
            vec![(3, "wireframe".to_string())],
            "the unguarded write-through is what f177 was"
        );
        let guarded = "    mut wireframe: ResMut<WireframeConfig>,\n\
                       ui.checkbox(&mut wireframe_on, \"Wireframe mode\");";
        assert!(resource_fields_handed_to_widgets(guarded, &res_mut_params(guarded)).is_empty());
        // A constructor takes its `&mut` before any `ui.` appears.
        let ctor = "    mut picker: ResMut<GatewayPicker>,\n\
                    egui::TextEdit::singleline(&mut picker.destination)";
        assert_eq!(
            resource_fields_handed_to_widgets(ctor, &res_mut_params(ctor)).len(),
            1
        );
        // A param bypassed at the top of its system is no longer a live
        // ResMut where the widgets are.
        let bypassed = "    mut picker: ResMut<GatewayPicker>,\n\
                        let picker = picker.bypass_change_detection();\n\
                        egui::TextEdit::singleline(&mut picker.destination)";
        assert!(res_mut_params(bypassed).is_empty());
        // A real mutation is not a widget write and stays allowed: the
        // resource genuinely changed, so its tick SHOULD move.
        assert!(
            resource_fields_handed_to_widgets(
                "    mut signals: ResMut<Signals>,\nlet f = std::mem::take(&mut signals.foreign);",
                &["signals".to_string()]
            )
            .is_empty(),
            "banning every &mut through a ResMut would ban the writes that mean it"
        );
        // A commented-out example is not a call.
        assert!(
            resource_fields_handed_to_widgets(
                "// ui.checkbox(&mut wireframe.global, \"x\")",
                &["wireframe".to_string()]
            )
            .is_empty()
        );

        let mut faults = Vec::new();
        let sources = rust_sources_under("src/ui");
        assert!(sources.len() > 20, "the walk found no sources to scan");
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("source is readable");
            let code = non_test_source(&source);
            let params = res_mut_params(code);
            for (line, param) in resource_fields_handed_to_widgets(code, &params) {
                faults.push(format!(
                    "{}:{line}: hands a widget `&mut {param}.…` straight through a \
                     ResMut — copy it into a local and write back on a change",
                    short(&path)
                ));
            }
        }
        assert!(faults.is_empty(), "{}", faults.join("\n  "));
    }

    /// UI copy uses one spelling of the words this product says most
    /// (#1264 f225).
    ///
    /// "Base color" on a plant, "Start colour" on particles and "Sun
    /// colour" in Environment; "Center X / Z" on a grid placement and
    /// "District centre (m)" on a road — inside one editor, on labels an
    /// owner reads hundreds of times a session. UK spelling won because
    /// the rest of the copy already leaned that way, and this is what
    /// keeps the next label from drifting back.
    ///
    /// Identifier-shaped literals are skipped: `"color_edit_button_rgb"`
    /// is an egui method name quoted inside another source scan, not
    /// something anybody reads on screen.
    #[test]
    fn ui_copy_uses_one_spelling_of_colour_and_centre() {
        let mut sources = rust_sources_under("src/ui");
        sources.extend(rust_sources_under("src/editor_gizmo"));

        assert!(sources.len() > 20, "the walk found no sources to scan");

        // The control, for the same reason as above.
        assert_eq!(us_spelling("Base color"), Some("color"));
        assert_eq!(us_spelling("Center X / Z"), Some("Center"));
        assert_eq!(us_spelling("Base colour"), None);
        assert_eq!(us_spelling("Centre X / Z"), None);
        assert_eq!(
            us_spelling("color_edit_button_rgb"),
            None,
            "an identifier quoted in another scan is not UI copy"
        );

        let mut drift = Vec::new();
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("source is readable");
            for literal in string_literals(non_test_source(&source)) {
                if let Some(wrong) = us_spelling(&literal) {
                    drift.push(format!("{}: {wrong:?} in {literal:?}", short(&path)));
                }
            }
        }
        assert!(
            drift.is_empty(),
            "US spellings in UI copy — this product says \"colour\" and \"centre\":\n  {}",
            drift.join("\n  ")
        );
    }

    // ---------------------------------------------------------------
    // The settled product vocabulary (#1266). Four owner decisions, four
    // scans, because each has a different scope and a different reason.
    //
    // The decisions, made 2026-09-05 after five names for one concept
    // shipped side by side:
    //
    // **The place is a `world`.** "Overlands" survives only as the product
    // name — wordmark, splash, OAuth pages, "a newer version of
    // Overlands". "room" stays on the wire, where it is the schema's own
    // noun, and never reaches a label.
    //
    // **The buildable is an `item` and a node inside one is a `part`.**
    // Region Asset, generator, blueprint and "stash item" were the other
    // four. The accepted cost is that "item" already means an Inventory
    // entry, so the scene menu's two destructive deletes are disambiguated
    // by SCOPE ("Delete this part" / "Delete the whole item") rather than
    // by noun.
    //
    // **The write is `Save`, and `PDS` is gone from every user-visible
    // string.** Not merely from the button: "Publish & travel" in the
    // unsaved guard was the same action under a verb the user had never
    // been taught, on the one dialog that stands between them and losing
    // work.
    //
    // **Worn things are `Wearables`.**
    //
    // Why scans and not just a sweep: "stash" was five strings when the
    // review found it, six by the time it was triaged and EIGHT by the
    // time it was swept — the extras added by tranches worked in between,
    // by people (me) who had read the finding. A vocabulary decision that
    // is only written down in prose is a vocabulary decision that drifts.

    /// A literal that is not UI copy: an egui id salt, a storage key, a
    /// metric name, an env-var name, a method name quoted inside another
    /// scan.
    ///
    /// The rule is shape, not a list: copy is written for a reader, so it
    /// either contains a space or is a single capitalised word ("Items",
    /// "Wearables", "Generators" — the tab and heading names this decision
    /// is mostly about). An identifier is lower-or-upper-case joined by
    /// `_`, `-`, `.`, `/` or `:` with no space. `ui.label("socket")` — a
    /// bare lowercase word with no separator — is deliberately COPY, and
    /// deliberately so: it is a real label on a real panel.
    fn is_ui_copy(literal: &str) -> bool {
        if literal.trim().is_empty() {
            return false;
        }
        if literal.contains(' ') {
            return true;
        }
        !literal.contains(['_', '-', '.', '/', ':'])
    }

    /// The sources every vocabulary scan walks: the UI, the in-world
    /// editor menus, and the modules outside both that hand a UI surface a
    /// string to print verbatim.
    fn ui_copy_sources() -> Vec<std::path::PathBuf> {
        let mut sources: Vec<std::path::PathBuf> = EXTRA_LABEL_SOURCES
            .iter()
            .map(|rel| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .collect();
        sources.extend(rust_sources_under("src/ui"));
        sources.extend(rust_sources_under("src/editor_gizmo"));
        assert!(sources.len() > 20, "the walk found no sources to scan");
        sources
    }

    /// Every UI-copy literal in `path`, with test code and log macros cut
    /// and format placeholders blanked.
    ///
    /// A `{…}` group is an expression, not words: nobody reads
    /// `MAX_AVATAR_ATTACHMENTS` in "All {MAX_AVATAR_ATTACHMENTS} slots are
    /// full", they read a number. Leaving them in makes every scan trip
    /// over its own subject's identifier name.
    fn copy_literals(path: &std::path::Path) -> Vec<String> {
        let source = std::fs::read_to_string(path).expect("source is readable");
        string_literals(&without_log_macros(non_test_source(&source)))
            .into_iter()
            .filter(|l| is_ui_copy(l))
            .map(|l| without_placeholders(&l))
            .collect()
    }

    /// `literal` with every `{…}` group replaced by a space.
    fn without_placeholders(literal: &str) -> String {
        let mut out = String::with_capacity(literal.len());
        let mut depth = 0usize;
        for c in literal.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// Run one vocabulary rule over the UI sources and report every drift.
    fn assert_no_drift(rule: fn(&str) -> Option<&'static str>, headline: &str) {
        let mut drift = Vec::new();
        for path in ui_copy_sources() {
            for literal in copy_literals(&path) {
                if let Some(why) = rule(&literal) {
                    drift.push(format!("{}: {why} — {literal:?}", short(&path)));
                }
            }
        }
        assert!(drift.is_empty(), "{headline}:\n  {}", drift.join("\n  "));
    }

    /// Whether `haystack` contains `needle` as a whole word.
    ///
    /// Needed because "Headroom kept between the camera and the terrain"
    /// is not about a room, and "part-way" is not about a part.
    fn has_word(haystack: &str, needle: &str) -> bool {
        let lower = haystack.to_lowercase();
        let needle = needle.to_lowercase();
        let mut from = 0;
        while let Some(at) = lower[from..].find(&needle) {
            let start = from + at;
            let end = start + needle.len();
            let before_ok = start == 0
                || !lower[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric());
            let after_ok = end == lower.len()
                || !lower[end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric());
            if before_ok && after_ok {
                return true;
            }
            from = end;
        }
        false
    }

    /// The place noun this literal drifted into, if any.
    fn stray_place_noun(literal: &str) -> Option<&'static str> {
        // The product name is not the place noun. Removed before the
        // check rather than special-cased after it, so "Enter the
        // Overlands" and "a newer version of Overlands" pass while
        // "Loading your overland" does not.
        let without_product = literal.replace("Overlands", "").replace("OVERLANDS", "");
        if has_word(&without_product, "overland") || has_word(&without_product, "overlands") {
            return Some("the place is a \"world\"");
        }
        // "room" is the WIRE's noun and stays there, so the rule is aimed
        // at prose: a determiner in front of it, a possessive after it, or
        // a label that opens with it. That leaves `timed_out("room
        // publish")`-style internal labels alone — and those are exactly
        // the strings that turned out to reach a toast, which is why they
        // were renamed rather than exempted.
        const PROSE: &[&str] = &[
            "this room",
            "the room",
            "a room",
            "your room",
            "their room",
            "my room",
            "target room",
            "room's",
            "rooms'",
            "in room",
        ];
        let lower = literal.to_lowercase();
        if PROSE.iter().any(|p| lower.contains(p))
            || lower.starts_with("room ")
            || lower.starts_with("⚠ room ")
        {
            return Some("the place is a \"world\"; \"room\" is the wire's word");
        }
        None
    }

    /// The buildable-tree noun this literal drifted into, if any.
    fn stray_buildable_noun(literal: &str) -> Option<&'static str> {
        if has_word(literal, "region asset")
            || has_word(literal, "region assets")
            || literal.contains("region-asset")
            || literal.contains("Region Asset")
        {
            return Some("a buildable is an \"item\"");
        }
        if has_word(literal, "generator") || has_word(literal, "generators") {
            return Some("a buildable is an \"item\"");
        }
        if has_word(literal, "blueprint") || has_word(literal, "blueprints") {
            return Some("a buildable is an \"item\"");
        }
        if has_word(literal, "stash") || has_word(literal, "stashes") {
            return Some("the place things live is the \"inventory\"");
        }
        None
    }

    /// A user-visible "PDS", or the write verb that is not "Save".
    fn stray_save_vocabulary(literal: &str) -> Option<&'static str> {
        if literal.contains("PDS") {
            return Some("say \"your account\" or \"the stored copy\"");
        }
        for verb in [
            "publish",
            "publishes",
            "publishing",
            "published",
            "unpublished",
        ] {
            if has_word(literal, verb) {
                return Some("the write is \"Save\"");
            }
        }
        None
    }

    /// The worn-things noun, if it drifted.
    fn stray_worn_noun(literal: &str) -> Option<&'static str> {
        (has_word(literal, "attachment") || has_word(literal, "attachments"))
            .then_some("worn things are \"wearables\"")
    }

    #[test]
    fn ui_copy_calls_the_place_a_world() {
        // Controls. Each scan carries the sentence that USED to ship and
        // the one that ships now, so a rule that stopped seeing anything
        // fails here rather than passing forever (#1264's lesson).
        assert!(stray_place_noun("Loading your overland — @{}").is_some());
        assert!(stray_place_noun("Travel to {}'s overland").is_some());
        assert!(stray_place_noun("Travel to a mutual follow of this room's owner").is_some());
        assert!(stray_place_noun("Contact effects from the room you're in:").is_some());
        assert!(stray_place_noun("Room theme").is_some());
        assert!(stray_place_noun("Loading your world — @{}").is_none());
        assert!(
            stray_place_noun("Enter the Overlands").is_none(),
            "the product name is not the place noun"
        );
        assert!(
            stray_place_noun("This item was authored by a newer version of Overlands").is_none()
        );
        assert!(
            stray_place_noun("Make room for it").is_none(),
            "the idiom is not the noun"
        );
        assert!(
            stray_place_noun("Headroom kept between the camera and the terrain").is_none(),
            "whole words only"
        );

        assert_no_drift(
            stray_place_noun,
            "UI copy that is not about a \"world\" — the place has one name",
        );
    }

    #[test]
    fn ui_copy_calls_a_buildable_an_item() {
        assert!(stray_buildable_noun("Region Assets").is_some());
        assert!(stray_buildable_noun("Rename Generator").is_some());
        assert!(stray_buildable_noun("Stored Generators: {count}/{cap}").is_some());
        assert!(stray_buildable_noun("Inventory — your saved item blueprints").is_some());
        assert!(stray_buildable_noun("Delete this item from your stash").is_some());
        assert!(stray_buildable_noun("Items").is_none());
        assert!(stray_buildable_noun("Delete this item from your inventory").is_none());
        assert!(
            stray_buildable_noun("Delete the whole item (and its placements)").is_none(),
            "the two deletes are told apart by SCOPE, not by a second noun"
        );

        assert_no_drift(
            stray_buildable_noun,
            "UI copy naming the buildable something other than an \"item\" (its child is a \"part\")",
        );
    }

    /// Scope note: `src/ui/login` is exempt because the login screen owns
    /// the PDS override field itself — its label, its validation and the
    /// errors that point at it. An operator field has to name the thing it
    /// configures. Everything else in the app is a user surface.
    #[test]
    fn ui_copy_says_save_and_never_says_pds() {
        assert!(stray_save_vocabulary("Save to PDS").is_some());
        assert!(stray_save_vocabulary("Publish & travel").is_some());
        assert!(stray_save_vocabulary("You have unpublished edits to: {}.").is_some());
        assert!(stray_save_vocabulary("Saving would overwrite the stored copy").is_none());
        assert!(stray_save_vocabulary("Save & travel").is_none());
        assert!(
            stray_save_vocabulary("Republishing").is_none(),
            "whole words only"
        );

        let mut drift = Vec::new();
        for path in ui_copy_sources() {
            if path.components().any(|c| c.as_os_str() == "login") {
                continue;
            }
            for literal in copy_literals(&path) {
                if let Some(why) = stray_save_vocabulary(&literal) {
                    drift.push(format!("{}: {why} — {literal:?}", short(&path)));
                }
            }
        }
        assert!(
            drift.is_empty(),
            "UI copy naming the write something other than \"Save\", or saying \"PDS\" \
             outside the login screen's operator field:\n  {}",
            drift.join("\n  ")
        );
    }

    #[test]
    fn ui_copy_calls_worn_things_wearables() {
        assert!(stray_worn_noun("Attachments").is_some());
        assert!(
            stray_worn_noun("Vehicles carry no attachments — pilot a body to wear this.").is_some()
        );
        assert!(stray_worn_noun("Wearables").is_none());

        assert_no_drift(
            stray_worn_noun,
            "UI copy calling worn things \"attachments\"",
        );
    }

    /// Every anomaly rule's two sentences are UI copy, so they answer to
    /// the same vocabulary the rest of the app does (#1271 f409).
    ///
    /// `ui::diagnostics` renders `RuleHeader::description` verbatim in the
    /// Active Anomalies strip and beside every per-metric pill, and hangs
    /// `technical` on the hover. Neither string lives under `src/ui`, so
    /// none of the four scans above could ever see them — and it showed:
    /// the shipped set said "a PDS record fetch exhausted its retry
    /// budget" and "relay reported peers in the room but no WebRTC data
    /// channel opened (offer glare or ICE/NAT failure)", to a user who
    /// clicked the toolbar's alarm dot expecting to be told what was
    /// wrong.
    ///
    /// The plain-vs-precise split is `technical`'s job; the product's own
    /// words are not negotiable on either side, so both are scanned.
    #[test]
    fn rule_prose_is_ui_copy() {
        // Controls: the two sentences that actually shipped.
        assert!(stray_save_vocabulary("a PDS record fetch exhausted its retry budget").is_some());
        assert!(
            stray_place_noun("relay reported peers in the room but no data channel opened")
                .is_some()
        );

        let registry = crate::diagnostics::anomaly::default_registry();
        let checks: [fn(&str) -> Option<&'static str>; 5] = [
            stray_place_noun,
            stray_buildable_noun,
            stray_save_vocabulary,
            stray_worn_noun,
            us_spelling,
        ];
        let mut drift = Vec::new();
        let mut checked = 0usize;
        for rule in registry.rules() {
            let h = rule.header();
            let both = [
                ("description", Some(h.description)),
                ("technical", h.technical),
            ];
            for (field, literal) in both {
                let Some(literal) = literal else { continue };
                checked += 1;
                for check in checks {
                    if let Some(why) = check(literal) {
                        drift.push(format!("{}.{field}: {why} — {literal:?}", h.id));
                    }
                }
            }
        }
        assert!(
            checked > 30,
            "the registry handed back {checked} strings — the walk found nothing"
        );
        assert!(
            drift.is_empty(),
            "anomaly-rule prose the Diagnostics panel renders, in the wrong words:\n  {}",
            drift.join("\n  ")
        );

        // The glyph law reaches these strings too, and for the same reason
        // it could not before: `every_ui_label_glyph_is_in_the_base_font_set`
        // walks `src/ui`. A rule that reads well and renders as tofu is a
        // worse badge than the id it replaced.
        let atlas = BaseAtlas::new();
        let mut tofu = Vec::new();
        for rule in registry.rules() {
            let h = rule.header();
            for literal in [Some(h.description), h.technical].into_iter().flatten() {
                for c in literal.chars() {
                    if !c.is_ascii() && !atlas.draws(c) {
                        tofu.push(format!("{}: {c} U+{:04X}", h.id, u32::from(c)));
                    }
                }
            }
        }
        tofu.sort();
        tofu.dedup();
        assert!(
            tofu.is_empty(),
            "rule prose the bundled fonts cannot draw:\n  {}",
            tofu.join("\n  ")
        );
    }

    /// The helpers the four scans share, checked on the inputs that made
    /// them necessary.
    #[test]
    fn the_copy_filter_and_the_log_cut_do_what_the_scans_need() {
        assert_eq!(
            without_placeholders("All {MAX_AVATAR_ATTACHMENTS} slots are full"),
            "All  slots are full",
            "an interpolated identifier is not a word anybody reads"
        );
        assert!(is_ui_copy("Items"), "a bare capitalised word is a tab name");
        assert!(
            is_ui_copy("socket"),
            "a bare lowercase word is a real label"
        );
        assert!(is_ui_copy("Delete this part"));
        assert!(!is_ui_copy("room-recovery-reset"), "an egui id salt");
        assert!(!is_ui_copy("symbios_overlands_prefs_v1"), "a storage key");
        assert!(!is_ui_copy("app.symbios.room"), "an NSID");
        assert!(!is_ui_copy(""));

        // A log line is not copy, on one line or several.
        assert_eq!(
            string_literals(&without_log_macros("info!(\"Room record saved\");\n")).len(),
            0
        );
        let multi = "warn!(\n    \"Stored {} record could not be decoded\",\n    LABEL\n);\nlet x = \"kept\";\n";
        assert_eq!(
            string_literals(&without_log_macros(multi)),
            vec!["kept".to_string()],
            "a multi-line log macro goes whole and the code after it survives"
        );

        // And the lexer half: a backslash-continued literal is ONE
        // literal, which is where twelve of the fifteen "PDS" strings
        // lived (#1266).
        let continued = "let s = \"first half \\\n         second half\";\n";
        assert_eq!(string_literals(continued).len(), 1);
        assert!(string_literals(continued)[0].contains("second half"));

        // A URL is one literal, not a literal plus a comment.
        assert_eq!(
            string_literals("let u = \"https://bsky.social/xrpc\";\n"),
            vec!["https://bsky.social/xrpc".to_string()]
        );

        // The cut is at the test MODULE. A `#[cfg(test)]` on an ordinary
        // item mid-file used to end the scan there, taking the rest of
        // three real files with it.
        let with_a_test_only_item = concat!(
            "fn a() { \"kept\" }\n",
            "#[cfg(test)]\n",
            "const ONLY_FOR_TESTS: u8 = 1;\n",
            "fn b() { \"also kept\" }\n",
            "#[cfg(test)]\n",
            "mod tests { \"cut\" }\n",
        );
        let kept = non_test_source(with_a_test_only_item);
        assert!(
            kept.contains("also kept"),
            "the code after a test-only item"
        );
        assert!(!kept.contains("\"cut\""), "and the test module still goes");
        assert!(kept.contains("ONLY_FOR_TESTS"), "the item itself is code");
        // The control: the old rule cut at the first attribute, so this
        // is the string that used to disappear.
        assert_eq!(
            with_a_test_only_item
                .find("\n#[cfg(test)]")
                .map(|at| with_a_test_only_item[..at].contains("also kept")),
            Some(false),
            "cutting at the FIRST attribute is what lost it"
        );

        // A lifetime does not open a char literal, and a char literal
        // holding a quote does not open a string.
        assert_eq!(
            string_literals("fn f<'a>(x: &'a str) -> &'a str { \"kept\" }\n"),
            vec!["kept".to_string()]
        );
        assert_eq!(
            string_literals("let q = '\"'; let s = \"kept\";\n"),
            vec!["kept".to_string()]
        );
    }

    /// No shipped label carries a run of spaces from the source's own
    /// indentation (#1266).
    ///
    /// **This one has cost two tranches.** A Rust string continued with a
    /// trailing backslash is one string with no gap in it — but a Python
    /// triple-quoted heredoc, which is how a lot of this repo's bulk
    /// rewrites are done, reads that backslash as ITS OWN line
    /// continuation, joins the lines, and bakes the following indentation
    /// into the literal as real spaces. It ships as "the field below" plus
    /// thirty-eight spaces plus "to go to your own world instead", and
    /// `fmt`, `clippy` and every test pass. #1227 shipped one (found under
    /// #1233); #1269's audience line shipped one into the working tree
    /// before this existed.
    ///
    /// **Deliberately per LINE, not through
    /// [`string_literals`].** That lexer joins a continued literal across
    /// newlines with the source's indentation intact, so it reports every
    /// correctly-written multi-line string as a defect. What distinguishes
    /// the two is exactly whether the run of spaces is inside a single
    /// source line, which is a question only the raw line can answer.
    ///
    /// `src/diagnostics/analyze` legitimately pads columns in aligned CLI
    /// output; it is not a UI-copy source and is not walked.
    #[test]
    fn no_ui_label_carries_the_sources_own_indentation() {
        // The control, both ways round.
        assert!(gapped_literals("let s = \"a          b\";").len() == 1);
        assert!(gapped_literals("let s = \"a b\";").is_empty());
        assert!(
            gapped_literals("// a          b").is_empty(),
            "a comment ships nothing"
        );

        let mut found = Vec::new();
        for path in ui_copy_sources() {
            let source = std::fs::read_to_string(&path).expect("source is readable");
            for (n, line) in non_test_source(&source).lines().enumerate() {
                for literal in gapped_literals(line) {
                    found.push(format!("{}:{}: {literal:?}", short(&path), n + 1));
                }
            }
        }
        assert!(
            found.is_empty(),
            "labels carrying a run of spaces from the source's indentation — a \
             backslash continuation eaten by a heredoc:\n  {}",
            found.join("\n  ")
        );
    }

    /// Every literal ON THIS LINE holding four or more consecutive spaces
    /// between two non-space characters.
    fn gapped_literals(line: &str) -> Vec<String> {
        let code = line.split("//").next().unwrap_or("");
        let mut out = Vec::new();
        let mut chars = code.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut literal = String::new();
            loop {
                match chars.next() {
                    None | Some('"') => break,
                    Some('\\') => {
                        chars.next();
                    }
                    Some(other) => literal.push(other),
                }
            }
            if has_indent_run(&literal) {
                out.push(literal);
            }
        }
        out
    }

    /// Four or more spaces with a non-space on each side.
    fn has_indent_run(literal: &str) -> bool {
        let chars: Vec<char> = literal.chars().collect();
        let mut run = 0usize;
        let mut seen_non_space = false;
        for (i, c) in chars.iter().enumerate() {
            if *c == ' ' {
                if seen_non_space {
                    run += 1;
                }
                continue;
            }
            if run >= 4 && i < chars.len() {
                return true;
            }
            run = 0;
            seen_non_space = true;
        }
        false
    }

    /// Probe for authoring: which candidate icon glyphs the base set can
    /// actually draw. Run with `--no-capture` to read the table; kept as
    /// a test so the answer stays checkable when the font set changes.
    #[test]
    fn candidate_icon_glyphs_report_their_coverage() {
        let atlas = BaseAtlas::new();
        for c in [
            '⌖', '◇', '◆', '⊕', '✥', '⬚', '⇔', '⤡', '✋', '⬌', '↔', '⊞', '⊹', '⟐', '◎', '⬅', '➡',
            '⬆', '⬇', '◀', '▶', '▲', '▼', '❐', '❏', '⎘', '📋', '➕', '✚',
        ] {
            eprintln!("{c} U+{:04X}: {}", u32::from(c), atlas.draws(c));
        }
    }
}
