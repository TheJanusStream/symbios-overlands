//! Periodic refresh of the relay **service-auth** token (#714).
//!
//! [`get_relay_service_auth`] mints a short-lived (~60 s on bsky) JWT that the WebRTC
//! signaller presents to the relay on every (re)connect. It is fetched once at
//! login (see [`crate::ui::login::poll_complete_auth_task`]) and, without this
//! module, never renewed — so any reconnect (portal hop, dead-socket respawn,
//! network flap) more than a token-lifetime after login re-handshakes with an
//! **expired** token, and the relay rejects it with HTTP 401 (its `validate_exp`
//! hardening). On native the signaller fast-fails 4xx and backs off, reusing
//! the same dead token forever; on wasm the browser hides the status and the
//! blind-retry budget is exhausted. Either way the peer cannot (re)join the
//! room, which is why a full logout/login (re-issuing a fresh token) was the
//! only recovery.
//!
//! This module keeps [`TokenSourceRes`] continuously fresh by re-issuing the
//! service token on a fixed cadence ([`config::network::SERVICE_TOKEN_REFRESH_SECS`])
//! well inside its lifetime, so the token the signaller reads at reconnect time
//! is always valid. The refresh also proactively renews the underlying OAuth
//! access token when it is near expiry, so a long session self-heals the same
//! way authenticated PDS writes do (see [`super::refresh`]).

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::signaller::TokenSourceRes;

use crate::config;
use crate::oauth::OauthRefreshCtx;
use crate::state::RelayHost;

/// An in-flight service-token refresh. Its result is the fresh token string,
/// or an error to log (the next cadence tick retries).
#[derive(Component)]
pub struct ServiceTokenRefreshTask(Task<Result<String, String>>);

/// Response body of `com.atproto.server.getServiceAuth`.
#[derive(serde::Deserialize)]
struct GetServiceAuthResponse {
    token: String,
}

/// Mint a relay service-auth JWT: `com.atproto.server.getServiceAuth` with
/// `aud = did:web:<relay_host>` and `lxm =`
/// [`RELAY_SERVICE_LXM`](super::RELAY_SERVICE_LXM).
///
/// Replaces `bevy_symbios_multiuser::auth::get_service_auth` for all relay
/// token mints (#736): that helper sends no `lxm`, which the PDS treats as
/// a wildcard-method request — only grantable under the retired
/// `transition:generic` scope or an audience-pinned `rpc:*?aud=…` grant.
/// Passing the concrete `lxm` here is what lets the client's scope use a
/// wildcard *audience* instead, so one static client metadata document
/// serves every relay host. The relay itself ignores the token's `lxm`
/// claim; it validates `iss`/`exp`/`nbf`/`aud` only.
pub async fn get_relay_service_auth(
    session: &AtprotoSession,
    relay_host: &str,
) -> Result<String, String> {
    let mut url = url::Url::parse(&session.xrpc_url("com.atproto.server.getServiceAuth"))
        .map_err(|e| format!("getServiceAuth url: {e}"))?;
    url.query_pairs_mut()
        .append_pair("aud", &format!("did:web:{relay_host}"))
        .append_pair("lxm", super::RELAY_SERVICE_LXM);
    let resp = session
        .session
        .get(url.as_str())
        .await
        .map_err(|e| format!("getServiceAuth: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("getServiceAuth returned {status}: {body}"));
    }
    let parsed: GetServiceAuthResponse = resp
        .json()
        .await
        .map_err(|e| format!("getServiceAuth decode: {e}"))?;
    Ok(parsed.token)
}

/// When the next mint is due, from a wall-clock `now`.
fn next_refresh_at(now: i64) -> i64 {
    now + config::network::SERVICE_TOKEN_REFRESH_SECS as i64
}

/// Should a mint fire this tick? Pure so the cadence can be pinned without
/// standing up an OAuth client.
///
/// A socket reopen jumps the queue: a reconnect is the moment the credential
/// is about to be presented, and it is the only event that says so. An
/// in-flight mint suppresses both — a second task would race the first onto
/// the same `TokenSource`.
fn refresh_is_due(now: i64, next_at: i64, socket_reopened: bool, in_flight: bool) -> bool {
    !in_flight && (now >= next_at || socket_reopened)
}

/// Spawn a service-token refresh on a fixed cadence while a session is active,
/// unless one is already in flight. The freshly-minted token is installed by
/// [`poll_service_token_refresh`].
///
/// Runs unconditionally in `Update`; the `Option<Res<…>>` gates make it inert
/// until login installs the session/token/relay resources, and reset the
/// cadence on logout so the next login starts a fresh schedule.
///
/// # Why the cadence is on wall clock (#1216)
///
/// It used to schedule on `Res<Time>`, which is Bevy's *virtual* clock and
/// clamps each frame's delta to 250 ms. A shut laptop lid or a long-
/// backgrounded tab therefore advanced `elapsed` by a quarter second rather
/// than by the real gap, so `next_at` survived the sleep and the next mint
/// could be a further 45 seconds of *running* time away. The token it renews
/// lives about 60 s, so through that whole window every socket respawn
/// presented a credential minted before the sleep — the relay 401s it, the
/// plugin's backoff doubles toward its 60 s ceiling, and the user's first
/// minute back is a silently degraded world. Suspend/resume is how most
/// sessions end and restart, so this fired on essentially every resume.
///
/// [`crate::state::now_epoch_secs`] is the project's wasm-safe wall clock
/// (`std::time` panics on wasm32) and is already what chat timestamps use.
/// A resume now finds `now >= next_at` on the first frame back and mints
/// immediately.
///
/// A reopened socket forces one too: a reconnect is precisely the moment the
/// credential is about to be presented, and it is the one event that says so.
#[allow(clippy::too_many_arguments)]
pub fn schedule_service_token_refresh(
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<OauthRefreshCtx>>,
    relay_host: Option<Res<RelayHost>>,
    token_source: Option<Res<TokenSourceRes>>,
    mut reopened: MessageReader<bevy_symbios_multiuser::prelude::LocalSocketReopened>,
    in_flight: Query<(), With<ServiceTokenRefreshTask>>,
    mut next_at: Local<i64>,
    mut initialized: Local<bool>,
) {
    // Drained unconditionally so a reopen that arrives while logged out
    // cannot fire against the next session.
    let socket_reopened = reopened.read().count() > 0;
    let (Some(session), Some(refresh_ctx), Some(relay_host), Some(_token_source)) =
        (session, refresh_ctx, relay_host, token_source)
    else {
        // Logged out (or not yet logged in): re-arm so the next login begins a
        // fresh cadence rather than firing immediately off a stale timer.
        *initialized = false;
        return;
    };

    // The one line that decides which clock this cadence runs on.
    let now = crate::state::now_epoch_secs();
    if !*initialized {
        *initialized = true;
        *next_at = next_refresh_at(now);
        // Login has just minted one; the socket it opens a frame or two
        // later must not immediately mint a second.
        return;
    }
    if !refresh_is_due(now, *next_at, socket_reopened, !in_flight.is_empty()) {
        return;
    }
    *next_at = next_refresh_at(now);

    let session = session.clone();
    let refresh_ctx = refresh_ctx.clone();
    let relay_host = relay_host.0.clone();

    let pool = IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            // Keep the underlying OAuth access token fresh so the DPoP-signed
            // getServiceAuth call itself does not 401 on a long-idle session —
            // mirrors the write path's proactive refresh in `super::refresh`.
            if session.session.is_expired_jittered() {
                super::refresh::refresh_session(&session.session, &refresh_ctx).await?;
            }
            get_relay_service_auth(&session, &relay_host).await
        };
        crate::config::http::run_or(
            fut,
            Err(crate::config::http::timed_out("relay service-auth token")),
        )
        .await
    });
    commands.spawn(ServiceTokenRefreshTask(task));
}

/// Drain a finished [`ServiceTokenRefreshTask`] and install the fresh token
/// into [`TokenSourceRes`] so the next signaller (re)connect reads it. A failed
/// refresh is left for the next cadence tick to retry — the current token may
/// still be valid, and a transient PDS error must not tear anything down.
///
/// What it is no longer is *silent* (#1215). This is the upstream cause of the
/// most user-visible failure on this surface — a socket that will not come
/// back — and it was the one link in the chain with no instrumentation at all:
/// a lone `warn!` to a console nobody is reading. Because
/// [`names::NET_SIGNAL_AUTH_REJECTIONS`] only counts what the relay *refuses*,
/// a client that never mints a token to present looked, from every gauge and
/// every rule, exactly like a healthy one — and the offline analyzer had
/// nothing to correlate against the `RelayAuthRejected`s it would see
/// downstream. Now each failure carries a session-log event, a consecutive
/// count on a gauge (which `RelayTokenRefreshFailing` watches), and — once the
/// count means reconnection is genuinely impossible — one toast naming the
/// user's only remedy.
///
/// [`names::NET_SIGNAL_AUTH_REJECTIONS`]: crate::diagnostics::names::NET_SIGNAL_AUTH_REJECTIONS
#[allow(clippy::too_many_arguments)]
pub fn poll_service_token_refresh(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ServiceTokenRefreshTask)>,
    token_source: Option<Res<TokenSourceRes>>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
    mut consecutive: Local<u64>,
    mut alarmed: Local<bool>,
) {
    use crate::diagnostics::names;
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(token) => {
                if let Some(ts) = &token_source {
                    ts.0.set(Some(token));
                    debug!("relay service-auth token refreshed");
                }
                // A success clears the streak AND the alarm, so a client that
                // recovers can raise the toast again if it breaks later.
                *consecutive = 0;
                *alarmed = false;
                metrics.observe_gauge(names::NET_RELAY_TOKEN_REFRESH_FAILURES, 0.0);
            }
            Err(e) => {
                *consecutive += 1;
                warn!("relay service-auth token refresh failed (will retry): {e}");
                metrics.observe_gauge(names::NET_RELAY_TOKEN_REFRESH_FAILURES, *consecutive as f64);
                session_log.warn(
                    now,
                    crate::diagnostics::event::EventPayload::ServiceTokenRefreshFailed {
                        reason: e.clone(),
                        consecutive: *consecutive,
                    },
                );
                // One toast per outage, not one per tick: the schedule
                // re-arms unconditionally, so a permanently-expired refresh
                // token retries forever and would otherwise toast forever.
                if *consecutive >= config::network::SERVICE_TOKEN_FAILURES_BEFORE_ALARM && !*alarmed
                {
                    *alarmed = true;
                    toasts.error(
                        "Can't renew your connection credential — you may not be able to \
                         rejoin worlds until you sign in again.",
                        now,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::event::EventPayload;
    use crate::diagnostics::{MetricsRegistry, SessionLog, names};
    use crate::notify::Toasts;

    /// An app with the sinks the poll system writes to. Tasks are spawned
    /// only after this returns — `IoTaskPool` is initialised by the plugin
    /// build, and a task spawned before it panics.
    fn harness() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<MetricsRegistry>();
        app.init_resource::<SessionLog>();
        app.init_resource::<Toasts>();
        app.add_systems(Update, poll_service_token_refresh);
        app
    }

    /// Spawn a resolved refresh and drive the app until the poll system has
    /// consumed it. A single `update()` is not enough: `poll_once` returns
    /// `None` while the task is still being scheduled, so a fixed number of
    /// frames makes the streak count race the executor.
    fn land(app: &mut App, result: Result<String, String>) {
        let task = IoTaskPool::get().spawn(async move { result });
        app.world_mut().spawn(ServiceTokenRefreshTask(task));
        for _ in 0..2_000 {
            app.update();
            let mut q = app.world_mut().query::<&ServiceTokenRefreshTask>();
            if q.iter(app.world()).next().is_none() {
                return;
            }
        }
        panic!("the refresh task never landed");
    }

    fn failures(app: &App) -> Vec<u64> {
        app.world()
            .resource::<SessionLog>()
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::ServiceTokenRefreshFailed { consecutive, .. } => Some(*consecutive),
                _ => None,
            })
            .collect()
    }

    /// THE SEQUENCE (#1216 f405): shut the lid mid-session, open it the
    /// next morning. On the virtual clock the whole night advanced
    /// `elapsed` by a quarter second, so `next_at` survived the sleep and
    /// the token — which lives about 60 s — stayed stale for up to another
    /// 45 s of running time while every socket respawn presented it. On
    /// wall clock the gap is the gap, and the mint fires on the first frame
    /// back.
    #[test]
    fn a_sleep_makes_the_next_mint_due_immediately() {
        let armed_at = 1_000_000i64;
        let next_at = next_refresh_at(armed_at);
        let period = config::network::SERVICE_TOKEN_REFRESH_SECS as i64;
        assert_eq!(next_at, armed_at + period);

        // A frame later: not due. This is the healthy steady state.
        assert!(!refresh_is_due(armed_at + 1, next_at, false, false));
        // Eight hours of real time later — the whole point.
        assert!(refresh_is_due(armed_at + 8 * 3600, next_at, false, false));
        // And exactly at the cadence.
        assert!(refresh_is_due(next_at, next_at, false, false));
    }

    /// A reopened socket jumps the queue: the reconnect IS the moment the
    /// credential gets presented, so waiting out the rest of a cadence tick
    /// spends the reconnect on the old token.
    #[test]
    fn a_reopened_socket_forces_a_mint() {
        let now = 1_000_000i64;
        let next_at = next_refresh_at(now);
        assert!(!refresh_is_due(now, next_at, false, false));
        assert!(refresh_is_due(now, next_at, true, false));
        // …but never two at once: a second task would race the first onto
        // the same `TokenSource` and the loser's token would win.
        assert!(!refresh_is_due(now, next_at, true, true));
        assert!(!refresh_is_due(now + 10_000, next_at, false, true));
    }

    /// The regression this file exists to prevent is a revert to
    /// `Res<Time>`, and the predicates above cannot see which clock feeds
    /// them. Read the scheduler's own source: it must take its `now` from
    /// the wall clock and must not hold Bevy's virtual one at all.
    ///
    /// Source-scanning is the idiom `diagnostics::event`'s variant roster
    /// already uses here, and for the same reason — the property is about
    /// the code, not about a value it produces.
    #[test]
    fn the_cadence_reads_the_wall_clock_and_not_bevy_time() {
        let source = include_str!("service_token.rs");
        let body = source
            .split_once("pub fn schedule_service_token_refresh(")
            .expect("the scheduler is in this file")
            .1
            .split_once("\n}\n")
            .expect("its body is brace-balanced")
            .0;
        assert!(
            body.contains("crate::state::now_epoch_secs()"),
            "the cadence must be measured on the wall clock (#1216)"
        );
        assert!(
            !body.contains("time.elapsed"),
            "Res<Time> is the virtual clock: it clamps a frame delta to \
             250 ms, so a suspend advances it by a quarter second"
        );
        assert!(
            !body.contains("Res<Time>"),
            "the scheduler must not even take the virtual clock (#1216)"
        );
    }

    /// THE SEQUENCE: the PDS starts refusing `getServiceAuth`, tick after
    /// tick. The failure arm used to be a lone `warn!` — no metric, no
    /// session-log event, no rule — so a client that could never rejoin the
    /// relay looked, from every gauge, exactly like a healthy one, and the
    /// analyzer had nothing to correlate against the rejections that follow
    /// downstream (#1215 f403).
    #[test]
    fn a_failing_token_refresh_leaves_a_trail_and_eventually_says_so() {
        let alarm = config::network::SERVICE_TOKEN_FAILURES_BEFORE_ALARM;
        let mut app = harness();
        for _ in 0..alarm {
            land(&mut app, Err("HTTP 400 invalid_grant".to_owned()));
        }

        assert_eq!(
            failures(&app),
            (1..=alarm).collect::<Vec<_>>(),
            "every failure is on the record, and the streak is countable"
        );
        assert_eq!(
            app.world()
                .resource::<MetricsRegistry>()
                .gauge_latest(names::NET_RELAY_TOKEN_REFRESH_FAILURES),
            Some(alarm as f64),
        );
        assert_eq!(
            app.world().resource::<Toasts>().shown().len(),
            1,
            "exactly one toast for the outage — the cadence re-arms forever, \
             so one per tick would be a permanent stream"
        );

        // Two more failures: still one toast, and the streak keeps counting.
        for _ in 0..2 {
            land(&mut app, Err("HTTP 400 invalid_grant".to_owned()));
        }
        assert_eq!(app.world().resource::<Toasts>().shown().len(), 1);
        assert_eq!(failures(&app).last().copied(), Some(alarm + 2));
    }

    /// THE SEQUENCE: a transient PDS error, then a success. One failure must
    /// not alarm, and a recovery must clear the streak so the gauge stops
    /// holding the rule violated — and so a LATER outage can raise its own
    /// toast rather than being swallowed by the first one's latch.
    #[test]
    fn a_recovery_clears_the_streak_and_re_arms_the_alarm() {
        let alarm = config::network::SERVICE_TOKEN_FAILURES_BEFORE_ALARM;
        let mut app = harness();
        land(&mut app, Err("timeout".to_owned()));
        assert!(
            app.world().resource::<Toasts>().shown().is_empty(),
            "one hiccup is not an outage"
        );

        land(&mut app, Ok("jwt".to_owned()));
        assert_eq!(
            app.world()
                .resource::<MetricsRegistry>()
                .gauge_latest(names::NET_RELAY_TOKEN_REFRESH_FAILURES),
            Some(0.0),
        );

        for _ in 0..alarm {
            land(&mut app, Err("HTTP 400 invalid_grant".to_owned()));
        }
        assert_eq!(
            failures(&app).last().copied(),
            Some(alarm),
            "the streak restarted from the success, it did not resume at 2"
        );
        assert_eq!(
            app.world().resource::<Toasts>().shown().len(),
            1,
            "a new outage earns a new toast"
        );
    }
}
