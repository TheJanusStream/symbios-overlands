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
//!   CJK code point appears in chat, a peer handle, or the login feed
//!   ([`detect_cjk_need`]), the file is read (native) or fetched from
//!   the deploy origin (wasm) on the `IoTaskPool`, and
//!   [`poll_cjk_fetch`] swaps in a rebuilt `FontDefinitions` once —
//!   brief tofu-then-correct, per the 2026-07-17 lazy-fetch decision.
//!
//! `ctx.set_fonts` is a full atlas swap, so the state machine
//! ([`CjkFonts`]) guarantees it happens at most twice per session
//! (base install, CJK upgrade) — never per frame. A missing asset
//! (operator forgot to deploy the OTF) degrades to a logged warning and
//! the tofu stays: fonts are never worth blocking a session over.

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

/// Build the app's font set: Noto Sans primary, egui's embedded fonts
/// as tail, plus — once loaded — the CJK fallback at the very end of
/// both families.
fn build_font_definitions(cjk: Option<Vec<u8>>) -> egui::FontDefinitions {
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
    *installed = true;
}

/// Watch the strings that can carry arbitrary user text — chat, peer
/// handles, the login post feed — and kick off the CJK load the first
/// time one needs it. Every scan is change-gated, so the steady-state
/// cost is three change-tick reads per frame.
pub fn detect_cjk_need(
    mut cjk: ResMut<CjkFonts>,
    chat: Res<crate::state::ChatHistory>,
    feed: Res<crate::ui::login::LoginPostFeed>,
    changed_peers: Query<&crate::state::RemotePeer, Changed<crate::state::RemotePeer>>,
) {
    if !matches!(cjk.status, CjkStatus::Dormant) {
        return;
    }
    let mut hit = false;
    if chat.is_changed() {
        hit = chat
            .messages
            .iter()
            .any(|m| needs_cjk(&m.text) || needs_cjk(&m.author));
    }
    if !hit && feed.is_changed() {
        hit = feed
            .posts
            .iter()
            .any(|p| needs_cjk(&p.text) || needs_cjk(&p.author_handle));
    }
    if !hit {
        hit = changed_peers
            .iter()
            .any(|p| p.handle.as_deref().is_some_and(needs_cjk));
    }
    if hit {
        info!("CJK text sighted — loading the CJK font fallback");
        cjk.status = CjkStatus::Fetching;
        cjk.task = Some(spawn_cjk_load());
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
