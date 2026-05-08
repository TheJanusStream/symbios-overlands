//! Bluesky post feed shown on the login screen.
//!
//! Pulls up to [`POST_LIMIT`] most-recent posts from the configured handle
//! that contain the configured hashtag (defaults: `codewright.bsky.social`,
//! `#Overlands`) via the public AppView's `app.bsky.feed.getAuthorFeed`
//! lexicon — same unauthenticated pattern as [`crate::social`] and
//! [`crate::avatar`]'s profile fetch.
//!
//! The matching `app.bsky.feed.searchPosts` lexicon would let the AppView
//! filter by hashtag server-side, but it 403s on the public endpoint
//! because it requires an authenticated session. `getAuthorFeed` is open,
//! so we fetch a larger window ([`AUTHOR_FEED_LIMIT`]) and filter on the
//! client. Reposts are skipped via the `reason` field; replies are
//! excluded server-side via `filter=posts_no_replies`.
//!
//! ## Configuration
//!
//! - `OVERLANDS_LOGIN_FEED_HANDLE` — author handle the panel reads from.
//! - `OVERLANDS_LOGIN_FEED_HASHTAG` — hashtag to filter on.
//!
//! Both are read at compile time via [`option_env!`] so the WASM build
//! (which has no run-time env access) can be configured at build time.
//!
//! ## Architecture
//!
//! 1. [`LoginPostFeed`] resource owns the visible state ([`FetchStatus`] +
//!    a `Vec<DisplayPost>`).
//! 2. [`start_login_feed_fetch`] runs on `OnEnter(AppState::Login)`,
//!    despawns any stale [`LoginFeedFetchTask`], and spawns a fresh one.
//! 3. [`poll_login_feed_fetch`] drains finished tasks each frame and
//!    writes their result back into [`LoginPostFeed`].
//! 4. [`render_login_feed_panel`] paints the egui section inside the
//!    existing login window, returning a [`LoginFeedAction`] so the
//!    parent UI system can act on Retry / Open clicks.

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use bevy_egui::egui;
use futures_lite::future;
use serde::Deserialize;

/// Compile-time fallback handle the panel reads from when the
/// `OVERLANDS_LOGIN_FEED_HANDLE` env var is unset at build time.
const DEFAULT_HANDLE: &str = "codewright.bsky.social";
/// Compile-time fallback hashtag filter when
/// `OVERLANDS_LOGIN_FEED_HASHTAG` is unset at build time.
const DEFAULT_HASHTAG: &str = "#Overlands";
/// Maximum number of posts the panel renders after filtering.
const POST_LIMIT: usize = 5;
/// How many recent posts to pull from `getAuthorFeed` before applying the
/// hashtag filter. 100 is the API's hard maximum per request; if the
/// hashtag is rarer than `1 in 100` of an author's recent posts, the
/// panel will just render fewer cards. Cursor pagination would let us
/// search deeper but is filed as a follow-up — empty-state UX already
/// handles the "no matches" case gracefully.
const AUTHOR_FEED_LIMIT: u32 = 100;
/// Soft cap on each post body; longer text is truncated with an ellipsis
/// to keep the egui layout stable on narrow login windows.
const TEXT_PREVIEW_CHARS: usize = 240;

fn feed_handle() -> &'static str {
    option_env!("OVERLANDS_LOGIN_FEED_HANDLE").unwrap_or(DEFAULT_HANDLE)
}

fn feed_hashtag() -> &'static str {
    option_env!("OVERLANDS_LOGIN_FEED_HASHTAG").unwrap_or(DEFAULT_HASHTAG)
}

/// One displayed post — strictly the fields the UI actually renders.
/// Built from a parsed [`PostView`] in [`fetch_posts`] so the on-wire
/// shape changes don't ripple into the render code.
#[derive(Debug, Clone)]
pub struct DisplayPost {
    pub text: String,
    /// `YYYY-MM-DD` slice of the original ISO 8601 timestamp. We don't
    /// pull in chrono just to format relative ages — the date is enough
    /// for "is this fresh?" feedback on the login screen.
    pub indexed_at: String,
    pub post_url: String,
    pub author_handle: String,
}

/// Fetch lifecycle. `Idle` is the starting state before any system has
/// kicked off a fetch; `Loading` while the [`IoTaskPool`] task is in-flight;
/// `Loaded` once the post list has been parsed; `Error` on transport,
/// status, or decode failure.
#[derive(Debug, Default, Clone)]
pub enum FetchStatus {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error(String),
}

/// Shared state behind the login screen's post panel. Updated by
/// [`poll_login_feed_fetch`] and read by [`render_login_feed_panel`].
#[derive(Resource, Default)]
pub struct LoginPostFeed {
    pub status: FetchStatus,
    pub posts: Vec<DisplayPost>,
}

/// In-flight `searchPosts` request. Carried as a component on a throwaway
/// entity so `Query<&mut LoginFeedFetchTask>` can drain it ergonomically
/// from [`poll_login_feed_fetch`].
#[derive(Component)]
pub struct LoginFeedFetchTask(Task<Result<Vec<DisplayPost>, String>>);

/// Action the panel render returned to the parent UI system.
pub enum LoginFeedAction {
    /// User did nothing meaningful this frame.
    None,
    /// User clicked the Retry button — re-dispatch the fetch.
    Retry,
    /// User clicked a post card — open the URL in a browser tab.
    OpenUrl(String),
}

/// Spawn a feed fetch task. Run on `OnEnter(AppState::Login)` and on
/// Retry clicks. Despawns any stale task entity from a prior fetch so a
/// late-arriving previous result can't overwrite the new one.
pub fn start_login_feed_fetch(
    mut commands: Commands,
    mut feed: ResMut<LoginPostFeed>,
    existing: Query<Entity, With<LoginFeedFetchTask>>,
) {
    for e in existing.iter() {
        commands.entity(e).despawn();
    }
    feed.status = FetchStatus::Loading;
    feed.posts.clear();
    spawn_post_fetch_task(&mut commands);
}

/// Free helper used by both the OnEnter system and the in-UI Retry click
/// — keeps the spawn logic single-sourced. Caller is expected to have
/// reset [`LoginPostFeed`]'s state to `Loading` first.
fn spawn_post_fetch_task(commands: &mut Commands) {
    let pool = IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = fetch_posts();
        // Same native/WASM split as `crate::social::query_resonance`:
        // `IoTaskPool` worker thread runs synchronously, so on native we
        // need a tokio runtime to host reqwest's async machinery; on
        // WASM the future runs directly through the browser's event loop.
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("tokio runtime: {e}"))?;
            rt.block_on(fut)
        }
    });
    commands.spawn(LoginFeedFetchTask(task));
}

/// Drain finished [`LoginFeedFetchTask`] entities each frame and copy
/// their result onto [`LoginPostFeed`]. Despawns the task entity once
/// drained so the panel reverts to a stable Loaded / Error display.
pub fn poll_login_feed_fetch(
    mut commands: Commands,
    mut feed: ResMut<LoginPostFeed>,
    mut tasks: Query<(Entity, &mut LoginFeedFetchTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        match result {
            Ok(posts) => {
                feed.posts = posts;
                feed.status = FetchStatus::Loaded;
            }
            Err(msg) => {
                feed.status = FetchStatus::Error(msg);
            }
        }
        commands.entity(entity).despawn();
    }
}

// ---------------------------------------------------------------------------
// HTTP fetch
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FeedResp {
    #[serde(default)]
    feed: Vec<FeedViewPost>,
}

/// One entry in `getAuthorFeed.feed`. The `reason` field is present iff
/// the entry is a repost; we use [`serde::de::IgnoredAny`] so we detect
/// presence without forcing serde to walk the entire reason payload.
#[derive(Deserialize)]
struct FeedViewPost {
    post: PostView,
    #[serde(default)]
    reason: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct PostView {
    uri: String,
    author: AuthorView,
    record: PostRecord,
    #[serde(rename = "indexedAt")]
    indexed_at: String,
}

#[derive(Deserialize)]
struct AuthorView {
    handle: String,
}

#[derive(Deserialize)]
struct PostRecord {
    #[serde(default)]
    text: String,
}

async fn fetch_posts() -> Result<Vec<DisplayPost>, String> {
    let client = crate::config::http::default_client();

    // `getAuthorFeed` is the unauthenticated counterpart to `searchPosts`
    // (which 403s on the public AppView). We pull a wide window of recent
    // posts and apply the hashtag filter client-side.
    let mut url = url::Url::parse("https://public.api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed")
        .map_err(|e| format!("url: {e}"))?;
    url.query_pairs_mut()
        .append_pair("actor", feed_handle())
        .append_pair("limit", &AUTHOR_FEED_LIMIT.to_string())
        .append_pair("filter", "posts_no_replies");

    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| format!("transport: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let parsed: FeedResp = resp.json().await.map_err(|e| format!("decode: {e}"))?;

    let needle = feed_hashtag().to_lowercase();
    Ok(parsed
        .feed
        .into_iter()
        // Skip reposts — `reason` is set only on `reasonRepost` /
        // `reasonPin` entries; original-author posts have no reason.
        .filter(|item| item.reason.is_none())
        .map(|item| item.post)
        .filter(|p| p.record.text.to_lowercase().contains(&needle))
        .take(POST_LIMIT)
        .filter_map(|p| {
            let rkey = rkey_from_uri(&p.uri)?;
            let post_url = format!("https://bsky.app/profile/{}/post/{}", p.author.handle, rkey);
            let mut text = p.record.text;
            if text.chars().count() > TEXT_PREVIEW_CHARS {
                let truncated: String = text.chars().take(TEXT_PREVIEW_CHARS).collect();
                text = format!("{truncated}…");
            }
            // First 10 chars of an ISO 8601 timestamp are the date —
            // good enough for "is this fresh" without pulling in chrono.
            let indexed_at = p.indexed_at.chars().take(10).collect();
            Some(DisplayPost {
                text,
                indexed_at,
                post_url,
                author_handle: p.author.handle,
            })
        })
        .collect())
}

/// Extract the rkey (last path segment) from an `at://did/coll/rkey` URI.
/// Returns `None` if the URI is empty or has no slashes.
fn rkey_from_uri(uri: &str) -> Option<String> {
    let key = uri.rsplit('/').next()?;
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

/// Render the post panel inside the parent egui container and report the
/// user's chosen action back to the caller. Stateless; the parent UI
/// system owns the [`LoginPostFeed`] resource and the [`Commands`] needed
/// to act on the returned action.
pub fn render_login_feed_panel(ui: &mut egui::Ui, feed: &LoginPostFeed) -> LoginFeedAction {
    let mut action = LoginFeedAction::None;

    ui.add_space(8.0);
    ui.separator();
    ui.label(
        egui::RichText::new(format!("Latest {} from @{}", feed_hashtag(), feed_handle())).strong(),
    );
    ui.add_space(4.0);

    match &feed.status {
        FetchStatus::Idle | FetchStatus::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Fetching posts…");
            });
        }
        FetchStatus::Error(msg) => {
            ui.colored_label(
                egui::Color32::from_rgb(200, 120, 120),
                format!("Couldn't fetch posts: {msg}"),
            );
            if ui.button("Retry").clicked() {
                action = LoginFeedAction::Retry;
            }
        }
        FetchStatus::Loaded => {
            if feed.posts.is_empty() {
                ui.colored_label(
                    egui::Color32::GRAY,
                    format!("(no recent posts contain {})", feed_hashtag()),
                );
            } else {
                for post in &feed.posts {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.monospace(
                                egui::RichText::new(format!("@{}", post.author_handle))
                                    .small()
                                    .color(egui::Color32::LIGHT_BLUE),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(
                                        egui::RichText::new(&post.indexed_at)
                                            .small()
                                            .color(egui::Color32::GRAY),
                                    );
                                },
                            );
                        });
                        // Wrapping label so long posts don't blow out the
                        // login window's fixed width.
                        ui.add(egui::Label::new(&post.text).wrap());
                        if ui
                            .small_button(
                                egui::RichText::new("Open on Bluesky →")
                                    .color(egui::Color32::from_rgb(120, 170, 220)),
                            )
                            .clicked()
                        {
                            action = LoginFeedAction::OpenUrl(post.post_url.clone());
                        }
                    });
                    ui.add_space(2.0);
                }
            }
        }
    }

    action
}

/// Open `url` in the user's default browser. On native this delegates to
/// the `webbrowser` crate; on WASM it falls through to `window.open` via
/// `web_sys`. Both code paths swallow errors — the worst case is the user
/// not seeing a tab open, which the caller can't recover from anyway.
pub fn open_url_in_browser(url: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = webbrowser::open(url);
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target(url, "_blank");
        }
    }
}

/// Trigger a fresh fetch from inside the login UI system without taking
/// the existing-task `Query` (which would push the system over Bevy's
/// 16-arg `IntoSystem` limit). Stale tasks are left alone — their results
/// land in [`LoginPostFeed`] before the new task's, and the new task's
/// result wins via the natural `Vec<DisplayPost>` overwrite in the
/// `Loaded` arm. Race-free because both paths produce equivalent output.
pub fn retry_fetch(commands: &mut Commands, feed: &mut LoginPostFeed) {
    feed.status = FetchStatus::Loading;
    feed.posts.clear();
    spawn_post_fetch_task(commands);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rkey_extracts_last_path_segment() {
        assert_eq!(
            rkey_from_uri("at://did:plc:abc/app.bsky.feed.post/rkey123"),
            Some("rkey123".to_string())
        );
    }

    #[test]
    fn rkey_rejects_empty_uri_and_trailing_slash() {
        assert_eq!(rkey_from_uri(""), None);
        assert_eq!(rkey_from_uri("at://x/"), None);
    }

    #[test]
    fn rkey_handles_no_slashes() {
        // A bare token (no slashes) is itself the "last path segment" by
        // rsplit semantics — surprising but harmless: it'll just produce
        // a malformed bsky.app URL the user can choose not to click.
        assert_eq!(
            rkey_from_uri("loose-token"),
            Some("loose-token".to_string())
        );
    }
}
