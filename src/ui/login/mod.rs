//! OAuth 2.0 + DPoP login flow.
//!
//! Collects PDS, Relay Host, and an optional destination DID, then drives
//! the atproto OAuth authorization-code flow via [`crate::oauth`]. The
//! authenticated handle is learnt back from the authorization response,
//! so no handle input is needed from the user. The flow is target-specific:
//!
//! - **WASM** — `sessionStorage` carries the pending-auth blob across the
//!   page redirect; the callback lands back on the hosted page with
//!   `?code=&state=` and `check_wasm_callback` kicks off the code
//!   exchange on the next frame.
//! - **Native** — a background `tiny_http` loopback server catches the
//!   redirect; the pending-auth blob lives in
//!   `crate::oauth::NativePendingAuthRes` and [`poll_native_callback`]
//!   drains the channel.
//!
//! ## Sub-module map
//!
//! * [`begin`] — drains [`BeginAuthTask`]s and hands the resulting URL
//!   to the platform-specific browser-launch path.
//! * [`complete`] — drains [`CompleteAuthTask`]s, installs session
//!   resources, transitions to `Loading`. Also home to the shared
//!   `install_completed_session` + `spawn_complete_task` helpers.
//! * [`native_callback`] (native only) — polls the loopback callback
//!   channel and triggers the code exchange.
//! * `wasm_resume` (wasm only) — `?code=&state=` URL parser + persisted-
//!   session resume task + its drainer.
//! * [`posts`] — the login-screen Bluesky feed: recent `#Overlands` posts
//!   fetched unauthenticated via `app.bsky.feed.getAuthorFeed`.

mod begin;
mod complete;
mod errors;
#[cfg(not(target_arch = "wasm32"))]
mod native_callback;
mod posts;
mod validation;
#[cfg(target_arch = "wasm32")]
mod wasm_resume;

pub use begin::poll_begin_auth_task;
pub use complete::poll_complete_auth_task;
#[cfg(not(target_arch = "wasm32"))]
pub use native_callback::poll_native_callback;
pub use posts::{LoginPostFeed, poll_login_feed_fetch, start_login_feed_fetch};
#[cfg(target_arch = "wasm32")]
pub use wasm_resume::{check_wasm_callback, check_wasm_resume, poll_resume_task};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::boot_params::BootParams;
use crate::oauth::{self, OauthClientRes, PendingAuth};
use crate::state::RelayHost;

/// (auth_url, pending)
type BeginOutcome = Result<(String, PendingAuth), String>;

/// Bundle returned by the fresh-login + resume async tasks. Replaces a
/// 4-tuple so adding the optional spawn pose did not turn the call sites
/// into positional-noise.
pub struct CompletedSession {
    pub session: AtprotoSession,
    pub refresh_ctx: crate::oauth::OauthRefreshCtx,
    pub service_token: String,
    pub room_did: String,
    /// Carried from the URL/CLI boot params (fresh login) or from the
    /// `BootParams` resource (resume). `None` ⇒ random spawn scatter.
    pub spawn_pos: Option<crate::boot_params::TargetPos>,
    pub spawn_yaw_deg: Option<f32>,
}

type CompleteOutcome = Result<CompletedSession, String>;

/// In-flight authorization initiation (PAR + `authorize`). On completion we
/// either navigate the tab (WASM) or launch the system browser (native).
#[derive(Component)]
pub struct BeginAuthTask(bevy::tasks::Task<BeginOutcome>);

/// In-flight `code` → token exchange + service-token round-trip, running
/// after the OAuth callback delivers an authorization code.
#[derive(Component)]
pub struct CompleteAuthTask(bevy::tasks::Task<CompleteOutcome>);

/// Latest login-pipeline failure, shown underneath the login form.
///
/// Kept as a Bevy `Resource` rather than a `Local` on either UI system so
/// the rendering system and the polling system share a single authoritative
/// buffer — a `Local<LoginError>` would give each system its own private
/// copy and silently swallow every message.
#[derive(Resource, Default)]
pub struct LoginError(pub Option<String>);

/// One-shot latches used by [`login_ui`] to drive the URL/CLI boot-param
/// pre-fill and the matching auto-submit. Tracked as a `Resource` rather
/// than `Local`s on the UI system because `Local`s persist for the whole
/// app lifetime: once a user logged in once with `boot.autosubmit=true`,
/// logged out, and returned to the login screen, the `Local`-backed
/// flags would still read `true`, and a still-valid `boot.autosubmit`
/// would silently fail to refire. Resetting this resource on
/// [`reset_login_ui_latch`] (run on `OnEnter(AppState::Login)`) lets a
/// re-entry behave the same as a fresh page load without forcing the
/// user to reload.
#[derive(Resource, Default)]
pub struct LoginUiLatch {
    /// Set the first frame the form copies values from `BootParams`.
    /// After that, `BootParams` is ignored so user edits to the form
    /// fields aren't silently overwritten by a re-render.
    pub prefilled: bool,
    /// Set the first frame the form fires the auto-submit (when
    /// `BootParams::autosubmit` is set). Latched so a re-render before
    /// the [`BeginAuthTask`] entity becomes visible doesn't double-fire.
    pub autosubmitted: bool,
    /// Set the first frame the idle form gives keyboard focus to the
    /// destination field (#848), so the type-then-Enter reflex works
    /// without a mouse. One-shot so later frames don't steal focus back
    /// from wherever the user tabbed to.
    pub focused: bool,
}

/// Reset the [`LoginUiLatch`] when the app (re)enters
/// [`crate::state::AppState::Login`]. Fires on initial state entry too,
/// which is harmless: the resource starts at default already. The
/// load-bearing case is the *re-entry* after logout — without this,
/// `BootParams` would never refire `autosubmit` for the second visit.
pub fn reset_login_ui_latch(mut latch: ResMut<LoginUiLatch>) {
    *latch = LoginUiLatch::default();
}

/// Native-only bundle of the loopback-listener resources [`login_ui`]
/// needs for the browser-waiting state (#847): presence of the receiver
/// marks the stretch between browser launch and callback, the server
/// handle powers *Cancel*, and the retained URL powers *Copy login URL*.
/// Bundled as a [`SystemParam`] struct to stay clear of Bevy's 16-param
/// `IntoSystem` ceiling, which already bites `login_ui` (see
/// [`posts::retry_fetch`]).
#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemParam)]
pub struct NativeWaitState<'w> {
    receiver: Option<Res<'w, oauth::NativeCallbackReceiver>>,
    server: Option<ResMut<'w, oauth::NativeCallbackServerRes>>,
    auth_url: Option<Res<'w, oauth::NativeAuthUrl>>,
}

/// WASM-only bundle for the persisted-session resume state (#847): the
/// in-flight [`wasm_resume::ResumeAuthTask`]s, so [`login_ui`] can show
/// "Resuming session…" instead of a fully-clickable form racing the
/// resume, plus the escape hatch that cancels it.
#[cfg(target_arch = "wasm32")]
#[derive(SystemParam)]
pub struct WasmResumeState<'w, 's> {
    resume_tasks: Query<'w, 's, Entity, With<wasm_resume::ResumeAuthTask>>,
}

#[derive(Clone)]
pub struct LoginFormState {
    pds: String,
    relay_host: String,
    target_did: String,
}

impl Default for LoginFormState {
    fn default() -> Self {
        Self {
            pds: crate::config::login::DEFAULT_PDS.into(),
            relay_host: crate::config::login::DEFAULT_RELAY_HOST.into(),
            target_did: crate::config::login::DEFAULT_TARGET_DID.into(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn login_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut form: Local<LoginFormState>,
    mut latch: ResMut<LoginUiLatch>,
    boot: Option<Res<BootParams>>,
    login_error: Res<LoginError>,
    oauth_client: Res<OauthClientRes>,
    begin_tasks: Query<Entity, With<BeginAuthTask>>,
    complete_tasks: Query<Entity, With<CompleteAuthTask>>,
    mut feed: ResMut<LoginPostFeed>,
    theme: Res<crate::ui::theme::CurrentTheme>,
    attract: Option<Res<crate::attract::AttractScene>>,
    terrain_mesh: Query<(), With<crate::terrain::TerrainMesh>>,
    #[cfg(not(target_arch = "wasm32"))] mut native: NativeWaitState,
    #[cfg(target_arch = "wasm32")] wasm: WasmResumeState,
) {
    // First-frame pre-fill from URL/CLI boot params. Done as a one-shot
    // (`latch.prefilled`) so a subsequent re-render does not stomp on
    // edits the user made after landing on the form. `pds` / `relay`
    // fall back to the form defaults when not provided so an empty boot
    // input behaves identically to the prior release.
    if !latch.prefilled
        && let Some(boot) = boot.as_deref()
        && boot.is_any()
    {
        if let Some(did) = &boot.target_did {
            form.target_did = did.clone();
        }
        if let Some(pds) = &boot.pds {
            form.pds = pds.clone();
        }
        if let Some(relay) = &boot.relay {
            form.relay_host = relay.clone();
        }
        latch.prefilled = true;
    }
    // egui `Context` is Arc-backed, so cloning it is cheap and lets us
    // paint the hero and both cards from this one system without holding
    // a `&mut EguiContexts` borrow across the `.show()` calls.
    let ctx = contexts.ctx_mut().unwrap().clone();

    use crate::config::ui::login as cfg;

    // Full-screen sky gradient behind everything (#896) — without it the
    // login screen floats over the raw `ClearColor`, which reads as an
    // unfinished tool rather than the doorway to a world. Once the
    // attract backdrop's demo world (#897) has a terrain mesh to show,
    // the gradient steps aside and the world takes over.
    let world_backdrop_visible = attract.is_some() && !terrain_mesh.is_empty();
    if !world_backdrop_visible {
        paint_backdrop(&ctx, &theme.0);
    }

    let screen = ctx.content_rect();

    // Hero wordmark + tagline, centred. Mirrors the HTML loading
    // screen's letterspaced teal wordmark so loading → login reads as
    // one continuous brand surface instead of a visual-language reset.
    // Over the live world backdrop the hero text needs its own contrast
    // guarantee — a translucent panel of the theme's window fill. Over
    // the flat gradient (whose colours the theme already vouches for)
    // it stays chromeless.
    let hero_frame = if world_backdrop_visible {
        egui::Frame::new()
            .fill(theme.0.window_fill.gamma_multiply(0.85))
            .corner_radius(10.0)
            .inner_margin(12.0)
    } else {
        egui::Frame::new()
    };
    let hero = egui::Area::new(egui::Id::new("login-hero"))
        .anchor(
            egui::Align2::CENTER_TOP,
            [0.0, screen.height() * cfg::HERO_TOP_FRAC],
        )
        .show(&ctx, |ui| {
            hero_frame.show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(letterspace("OVERLANDS"))
                            .size(cfg::WORDMARK_TEXT_SIZE)
                            .strong()
                            .color(theme.0.accent),
                    );
                    ui.add_space(6.0);
                    // Two labels, not one wrapping label (#898): a wrap
                    // just before the final word orphaned "friends." on
                    // its own line, and each label centres itself.
                    ui.label(
                        egui::RichText::new("Procedurally seeded worlds on ATProto")
                            .size(cfg::TAGLINE_TEXT_SIZE)
                            .color(theme.0.text_weak),
                    );
                    ui.label(
                        egui::RichText::new("Explore, build, visit friends.")
                            .size(cfg::TAGLINE_TEXT_SIZE)
                            .color(theme.0.text_weak),
                    );
                });
            });
        });

    // Card-pair geometry, computed fresh from the live screen rect each
    // frame: the pair centres as a group, cards shrink on cramped
    // viewports, and the feed card drops underneath the login card when
    // the two can't sit side by side.
    let margin2 = 2.0 * cfg::CARD_INNER_MARGIN;
    let max_card_w = (screen.width() - 2.0 * cfg::EDGE_PAD - margin2).max(120.0);
    let login_w = cfg::CARD_WIDTH.min(max_card_w);
    let feed_w = cfg::FEED_CARD_WIDTH.min(max_card_w);
    let login_outer = login_w + margin2;
    let feed_outer = feed_w + margin2;
    let pair_w = login_outer + cfg::CARD_GUTTER + feed_outer;
    let stacked = pair_w + 2.0 * cfg::EDGE_PAD > screen.width();
    let cards_top =
        (screen.height() * cfg::CARDS_TOP_FRAC).max(hero.response.rect.bottom() + cfg::CARD_GUTTER);
    let login_x = if stacked {
        screen.center().x - login_outer / 2.0
    } else {
        screen.center().x - pair_w / 2.0
    };

    let login_resp = egui::Area::new(egui::Id::new("login-card"))
        .fixed_pos(egui::pos2(login_x, cards_top))
        .show(&ctx, |ui| {
            card_frame(&theme.0).show(ui, |ui| {
                ui.set_width(login_w);
                ui.label(
                    egui::RichText::new("Sign in with your Bluesky (ATProto) account.")
                        .color(theme.0.text_weak),
                );
                ui.add_space(10.0);

                // Enter-to-submit (#848): a field that just lost focus to the
                // Enter key reads as "I'm done typing — go".
                let mut enter_submitted = false;
                let mut track_enter = |resp: &egui::Response| {
                    if resp.lost_focus() && resp.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        enter_submitted = true;
                    }
                };

                ui.label(egui::RichText::new("Destination").strong());
                let dest_resp = ui.add(
                    egui::TextEdit::singleline(&mut form.target_did)
                        .hint_text("@friend.bsky.social — blank for your own world")
                        .desired_width(f32::INFINITY),
                );
                track_enter(&dest_resp);
                if !latch.focused {
                    dest_resp.request_focus();
                    latch.focused = true;
                }

                // The PDS / relay endpoints are operator plumbing nobody
                // should touch on a first login — folded away so the first
                // screen doesn't lead with a bare IP that reads as sketchy.
                ui.collapsing("Advanced", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("PDS:");
                        track_enter(&ui.text_edit_singleline(&mut form.pds));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Relay Host:");
                        track_enter(&ui.text_edit_singleline(&mut form.relay_host));
                    });
                });

                ui.add_space(8.0);

                let redirecting = !begin_tasks.is_empty();
                let completing = !complete_tasks.is_empty();
                // Target-specific third busy state (#847): on native, the
                // stretch between browser launch and loopback callback; on
                // WASM, the silent persisted-session resume that used to
                // hide behind a fully-clickable form.
                #[cfg(not(target_arch = "wasm32"))]
                let waiting = native.receiver.is_some();
                #[cfg(target_arch = "wasm32")]
                let waiting = !wasm.resume_tasks.is_empty();
                let mut begin_now = false;
                if !redirecting && !completing && !waiting {
                    // Primary call to action — full card width, oversized,
                    // and filled with the identity accent (#855, teal — was
                    // a one-off green) so it reads as *the* thing to do on
                    // the login screen rather than a peer of the text fields.
                    let enter = ui.add_sized(
                        [ui.available_width(), cfg::ENTER_BUTTON_HEIGHT],
                        egui::Button::new(
                            egui::RichText::new("Enter the Overlands")
                                .size(cfg::ENTER_BUTTON_TEXT_SIZE)
                                .strong()
                                .color(theme.0.accent_fill_text),
                        )
                        .fill(theme.0.accent_fill),
                    );
                    if enter.clicked() || enter_submitted {
                        begin_now = true;
                    }
                    // Auto-submit when the URL/CLI supplied a destination DID.
                    // Latched on `latch.autosubmitted` so we never double-fire
                    // even if the form re-renders before the BeginAuthTask
                    // spawns. Only `did` triggers this; `pds` / `relay` alone
                    // pre-fill but leave the click to the user.
                    //
                    // On WASM, a persisted session resume is preferred: it
                    // skips the OAuth redirect entirely and `check_wasm_resume`
                    // already applies the URL `did=` override. Autosubmitting
                    // on top would spawn two competing auth tasks; suppress
                    // ourselves and let the resume path handle the link.
                    #[cfg(target_arch = "wasm32")]
                    let has_persisted = oauth::wasm::load_persisted().is_some();
                    #[cfg(not(target_arch = "wasm32"))]
                    let has_persisted = false;
                    if !latch.autosubmitted
                        && !has_persisted
                        && let Some(b) = boot.as_deref()
                        && b.autosubmit
                    {
                        begin_now = true;
                        latch.autosubmitted = true;
                    }
                    if !begin_now {
                        // Idle state — render nothing extra. The button above
                        // is the only affordance.
                    }
                } else if completing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Completing authentication…");
                    });
                    // Escape hatch for a hung exchange (#848). The
                    // authorization code is single-use, so a cancelled
                    // exchange can't be resumed — the user just starts a
                    // fresh login, which is exactly what the form offers.
                    if ui.button("Cancel").clicked() {
                        for e in complete_tasks.iter() {
                            commands.entity(e).despawn();
                        }
                        commands.insert_resource(LoginError(None));
                    }
                } else if redirecting {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Contacting your PDS…");
                    });
                    if ui.button("Cancel").clicked() {
                        // Dropping the task aborts the discovery round-trip.
                        for e in begin_tasks.iter() {
                            commands.entity(e).despawn();
                        }
                        commands.insert_resource(LoginError(None));
                    }
                } else {
                    // `waiting` — the target-specific stretch.
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Complete the login in your browser…");
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                // Shut the loopback listener down promptly
                                // (frees the port for the next attempt) and
                                // drop the rest of the attempt's resources.
                                if let Some(server) = native.server.as_mut()
                                    && let Some(mut handle) = server.0.take()
                                {
                                    handle.shutdown();
                                }
                                commands.remove_resource::<oauth::NativeCallbackReceiver>();
                                commands.remove_resource::<oauth::NativeCallbackServerRes>();
                                commands.remove_resource::<oauth::NativePendingAuthRes>();
                                commands.remove_resource::<oauth::NativeAuthUrl>();
                                commands.insert_resource(LoginError(None));
                            }
                            if let Some(url) = native.auth_url.as_deref()
                                && ui
                                    .button("Copy login URL")
                                    .on_hover_text(
                                        "Paste into any browser on this machine \
                                     to finish signing in",
                                    )
                                    .clicked()
                            {
                                ui.ctx().copy_text(url.0.clone());
                            }
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Resuming your previous session…");
                        });
                        ui.add_space(4.0);
                        if ui.button("Not you? Sign in differently").clicked() {
                            // Cancel the in-flight resume (dropping the task
                            // aborts it), forget the persisted session, and
                            // fall back to the idle form. Latch autosubmit
                            // off so a boot `did=` link doesn't immediately
                            // re-fire a login the user just backed out of.
                            oauth::wasm::clear_persisted();
                            for e in wasm.resume_tasks.iter() {
                                commands.entity(e).despawn();
                            }
                            latch.autosubmitted = true;
                            commands.insert_resource(LoginError(None));
                        }
                    }
                }
                // Validate at the form (#848) so a blank relay or typo'd
                // destination fails right here with a readable message,
                // instead of minutes later deep in the pipeline.
                if begin_now {
                    match validation::validate_form(&form.pds, &form.relay_host, &form.target_did) {
                        Err(msg) => {
                            commands.insert_resource(LoginError(Some(msg)));
                        }
                        Ok(validated) => {
                            commands.insert_resource(LoginError(None));
                            // Reflect the normalisation (scheme prepended,
                            // stray scheme stripped) back into the form so
                            // what runs is what the user sees.
                            form.pds = validated.pds_url.clone();
                            form.relay_host = validated.relay_host.clone();
                            let boot_pos = boot.as_deref().and_then(|b| b.target_pos);
                            let boot_yaw = boot.as_deref().and_then(|b| b.target_yaw_deg);
                            info!(
                                "OAuth begin: pds={} relay={} destination={:?}",
                                validated.pds_url, validated.relay_host, validated.destination
                            );
                            commands.insert_resource(RelayHost(validated.relay_host.clone()));

                            let client = oauth_client.0.clone();
                            let pool = bevy::tasks::IoTaskPool::get();
                            let task = pool.spawn(async move {
                                let validation::ValidatedForm {
                                    pds_url,
                                    relay_host,
                                    destination,
                                } = validated;
                                let fut = async move {
                                    let target_did = match destination {
                                        validation::Destination::Home => String::new(),
                                        validation::Destination::Did(did) => did,
                                        // An @handle destination resolves to a
                                        // DID up front — a typo fails in one
                                        // round-trip with a spelling hint,
                                        // instead of burning the post-login
                                        // record-fetch retry budget.
                                        validation::Destination::Handle(handle) => {
                                            let http = crate::config::http::default_client();
                                            crate::pds::resolve_handle(&http, &handle).await?
                                        }
                                    };
                                    let (auth_url, mut pending) = oauth::begin_authorization(
                                        &client,
                                        &pds_url,
                                        &relay_host,
                                        &target_did,
                                    )
                                    .await?;
                                    // Carry the URL/CLI spawn pose across the OAuth
                                    // redirect — the AS strips our query params, so
                                    // this is the only path that survives.
                                    pending.target_pos = boot_pos;
                                    pending.target_yaw_deg = boot_yaw;
                                    Ok::<_, String>((auth_url, pending))
                                };
                                #[cfg(target_arch = "wasm32")]
                                {
                                    fut.await
                                }
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    crate::config::http::block_on(fut)
                                }
                            });
                            commands.spawn(BeginAuthTask(task));
                        }
                    }
                }

                if let Some(err) = &login_error.0 {
                    let (friendly, details) = errors::friendly_login_error(err);
                    ui.colored_label(crate::ui::theme::current(ui.ctx()).status.error, friendly);
                    if let Some(raw) = details {
                        ui.collapsing("Details", |ui| {
                            ui.small(raw);
                        });
                    }
                }

                // A visitor without an ATProto account needs a path (#848);
                // account creation lives with Bluesky, not us.
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label("New here?");
                    if ui.link("Create a free Bluesky account").clicked() {
                        posts::open_url_in_browser(crate::config::login::SIGNUP_URL);
                    }
                });
            });
        });

    // Latest #Overlands posts from the configured Bluesky handle, in its
    // own card beside the login card (underneath it when a narrow
    // viewport stacks the pair). The render helper is action-driven so
    // this system owns the side-effects (browser open, fetch retry).
    let feed_pos = if stacked {
        egui::pos2(
            screen.center().x - feed_outer / 2.0,
            login_resp.response.rect.bottom() + cfg::CARD_GUTTER,
        )
    } else {
        egui::pos2(login_x + login_outer + cfg::CARD_GUTTER, cards_top)
    };
    egui::Area::new(egui::Id::new("login-feed-card"))
        .fixed_pos(feed_pos)
        .show(&ctx, |ui| {
            card_frame(&theme.0).show(ui, |ui| {
                ui.set_width(feed_w);
                ui.label(
                    egui::RichText::new(posts::feed_panel_title())
                        .size(cfg::FEED_HEADING_TEXT_SIZE)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(posts::feed_panel_subtitle())
                        .small()
                        .color(theme.0.text_weak),
                );
                ui.add_space(6.0);
                // Balanced-pair cap: never taller than the height
                // budget, never past the bottom of a short viewport.
                let avail_h =
                    (screen.bottom() - cfg::EDGE_PAD - ui.next_widget_position().y).max(120.0);
                let max_h = (screen.height() * cfg::FEED_MAX_HEIGHT_FRAC).min(avail_h);
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    // Load-bearing inside an auto-sized `Area` (#898): the
                    // area ui reports only a placeholder available height
                    // (~64 px), and the scroll viewport is `min(available,
                    // max_height)` floored by `min_scrolled_height` — so
                    // without raising that floor to the real budget, the
                    // feed collapses to a 64 px slit. Content shorter than
                    // the budget still auto-shrinks.
                    .min_scrolled_height(max_h)
                    .show(ui, |ui| match posts::render_login_feed_panel(ui, &feed) {
                        posts::LoginFeedAction::None => {}
                        posts::LoginFeedAction::Retry => {
                            posts::retry_fetch(&mut commands, &mut feed);
                        }
                        posts::LoginFeedAction::OpenUrl(url) => {
                            posts::open_url_in_browser(&url);
                        }
                    });
            });
        });
}

/// Insert a plain space between every character (word gaps widen to
/// three spaces): poor-man's letterspacing for the hero wordmark, since
/// egui has no tracking control. Plain ASCII spaces rather than U+2009
/// thin spaces keep the Noto glyph-coverage story trivial (#858).
fn letterspace(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Full-screen vertical gradient (zenith → horizon) painted on egui's
/// background layer, beneath every `Area`. Colours come from the
/// semantic theme ([`crate::ui::theme::Theme::backdrop_top`] /
/// [`crate::ui::theme::Theme::backdrop_bottom`]) so all three palettes
/// keep their contrast guarantees.
fn paint_backdrop(ctx: &egui::Context, theme: &crate::ui::theme::Theme) {
    let rect = ctx.content_rect();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), theme.backdrop_top);
    mesh.colored_vertex(rect.right_top(), theme.backdrop_top);
    mesh.colored_vertex(rect.right_bottom(), theme.backdrop_bottom);
    mesh.colored_vertex(rect.left_bottom(), theme.backdrop_bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ctx.layer_painter(egui::LayerId::background())
        .add(egui::Shape::mesh(mesh));
}

/// Shared chrome for the login-screen cards: a rounded, bordered,
/// softly drop-shadowed panel — deliberately *not* an [`egui::Window`],
/// so nothing on the pre-world screen looks draggable or closable.
fn card_frame(theme: &crate::ui::theme::Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.window_fill)
        .stroke(egui::Stroke::new(theme.border_stroke_width, theme.border))
        .corner_radius(crate::config::ui::login::CARD_CORNER_RADIUS)
        .inner_margin(crate::config::ui::login::CARD_INNER_MARGIN)
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 0,
            color: egui::Color32::from_black_alpha(80),
        })
}
