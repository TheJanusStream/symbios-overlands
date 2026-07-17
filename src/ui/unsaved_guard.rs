//! Confirm-dialog guard against silently losing unpublished record edits.
//!
//! Two flows used to discard in-flight edits without warning: walking
//! through an inter-room portal replaces [`LiveRoomRecord`] /
//! [`StoredRoomRecord`] with the destination owner's record, and logging
//! out tears down every record resource. Both now route through
//! [`UnsavedGuard`]: the portal contact handler and the Log out button
//! insert the resource instead of acting directly, and [`unsaved_guard_ui`]
//! decides what happens next.
//!
//! The guard is deliberately the *single* owner of the dirty logic:
//!
//! - **Clean case** — nothing relevant differs from its stored mirror, so
//!   the guard proceeds on the very next frame without rendering anything.
//!   Callers therefore never need the record resources themselves; they
//!   unconditionally open the guard.
//! - **Dirty case** — a modal offers *Publish & continue* (spawns the same
//!   publish tasks the editors use, waits for every poll to drain, then
//!   re-checks), *Discard & continue*, or *Stay*.
//!
//! Waiting for the publish to finish before acting is load-bearing, not
//! politeness: the publish poll systems pin `stored = live` **at completion
//! time**, so letting portal travel swap [`LiveRoomRecord`] while a publish
//! is in flight would pin the *destination's* record as the local user's
//! stored mirror the moment the task resolved.
//!
//! Which records are "relevant" depends on the action: portal travel only
//! swaps the room record (avatar and inventory ride along), so only room
//! dirt blocks it — and only when the local user actually owns the room
//! they are standing in. Logout discards everything, so room (owner only),
//! avatar and inventory all count.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::oauth::OauthRefreshCtx;
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::player::{PortalCooldown, begin_portal_travel};
use crate::state::{
    AppState, CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord,
    PublishFeedback, PublishStatus, StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord,
    records_differ,
};
use crate::ui::avatar::PublishAvatarTask;
use crate::ui::inventory::PublishInventoryTask;
use crate::ui::room::PublishRoomTask;

/// How long portal interaction stays suppressed after the player chooses
/// *Stay*. Longer than the post-teleport [`PortalCooldown`] default: the
/// player is standing inside the portal collider when they decline, and
/// the wider window gives them time to walk clear before the overlap
/// re-opens the dialog.
const DECLINE_COOLDOWN_SECS: f64 = 3.0;

/// What the guard will do once the dirty question is settled.
#[derive(Clone, Debug)]
pub enum GuardedAction {
    /// Begin the async room-record fetch that carries the player to
    /// another overland (see `player::begin_portal_travel`). `target_pos:
    /// None` arrives at the destination record's `default_landing` (#745).
    PortalTravel {
        target_did: String,
        target_pos: Option<Vec3>,
    },
    /// Transition back to `AppState::Login`; `logout::cleanup_on_logout`
    /// does the actual teardown on the state edge.
    Logout,
    /// Close the app (native window-close intercept, #839): the window's
    /// close button routes through this guard instead of killing the
    /// process with unsaved edits aboard. Confirming exits via `AppExit`.
    Quit,
}

/// Dialog lifecycle. `Publishing` renders a spinner and waits for every
/// outstanding publish task to drain before re-checking the dirty set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardPhase {
    Prompt,
    Publishing,
}

/// Present while a guarded action is pending. Inserted by the portal
/// contact handler and the Log out button; removed by [`unsaved_guard_ui`]
/// when the action proceeds or the user backs out (and defensively by
/// `logout::cleanup_on_logout`).
#[derive(Resource, Debug)]
pub struct UnsavedGuard {
    pub action: GuardedAction,
    pub phase: GuardPhase,
    /// Failure message from the most recent publish attempt, surfaced in
    /// the dialog so the user understands why they are being re-asked.
    pub error: Option<String>,
}

impl UnsavedGuard {
    pub fn new(action: GuardedAction) -> Self {
        Self {
            action,
            phase: GuardPhase::Prompt,
            error: None,
        }
    }
}

/// Which editable records currently differ from their stored mirrors.
/// `room` is owner-gated by the caller: a visitor's live room record
/// legitimately diverges whenever the host edits live, and that is not
/// the visitor's data to save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyRecords {
    pub room: bool,
    pub avatar: bool,
    pub inventory: bool,
}

impl DirtyRecords {
    /// True when this dirty set should block the given action. Portal
    /// travel only replaces the room record; logout discards everything.
    pub(crate) fn blocks(&self, action: &GuardedAction) -> bool {
        match action {
            GuardedAction::PortalTravel { .. } => self.room,
            GuardedAction::Logout | GuardedAction::Quit => {
                self.room || self.avatar || self.inventory
            }
        }
    }
}

/// The six live/stored record resources the dirty computation diffs.
#[derive(SystemParam)]
pub struct GuardRecords<'w> {
    live_room: Option<Res<'w, LiveRoomRecord>>,
    stored_room: Option<Res<'w, StoredRoomRecord>>,
    live_avatar: Option<Res<'w, LiveAvatarRecord>>,
    stored_avatar: Option<Res<'w, StoredAvatarRecord>>,
    live_inventory: Option<Res<'w, LiveInventoryRecord>>,
    stored_inventory: Option<Res<'w, StoredInventoryRecord>>,
}

impl GuardRecords<'_> {
    /// Diff every live record against its stored mirror. `owns_room`
    /// gates the room diff — see [`DirtyRecords`].
    fn compute(&self, owns_room: bool) -> DirtyRecords {
        let differ_room = owns_room
            && match (&self.live_room, &self.stored_room) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            };
        let differ_avatar = match (&self.live_avatar, &self.stored_avatar) {
            (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
            _ => false,
        };
        let differ_inventory = match (&self.live_inventory, &self.stored_inventory) {
            (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
            _ => false,
        };
        DirtyRecords {
            room: differ_room,
            avatar: differ_avatar,
            inventory: differ_inventory,
        }
    }
}

/// Publish-status lines for the three record types, written when the
/// guard spawns publish tasks and read back for the failure message.
#[derive(SystemParam)]
pub struct GuardFeedbacks<'w> {
    room: ResMut<'w, PublishFeedback<RoomRecord>>,
    avatar: ResMut<'w, PublishFeedback<AvatarRecord>>,
    inventory: ResMut<'w, PublishFeedback<InventoryRecord>>,
}

impl GuardFeedbacks<'_> {
    /// First failure message among the record types the action cares
    /// about, for the dialog's error line.
    fn failure_message(&self, action: &GuardedAction) -> Option<String> {
        let mut sources: Vec<(&str, &PublishStatus)> = vec![("World", &self.room.status)];
        if matches!(action, GuardedAction::Logout | GuardedAction::Quit) {
            sources.push(("Avatar", &self.avatar.status));
            sources.push(("Inventory", &self.inventory.status));
        }
        sources
            .into_iter()
            .find_map(|(label, status)| match status {
                PublishStatus::Failed { message, .. } => Some(format!("{label}: {message}")),
                _ => None,
            })
    }
}

/// Existence probes for the three publish-task components. The guard's
/// `Publishing` phase waits until all of them have drained (the editors'
/// poll systems despawn each task entity when its result lands).
#[derive(SystemParam)]
pub struct GuardPublishTasks<'w, 's> {
    room: Query<'w, 's, (), With<PublishRoomTask>>,
    avatar: Query<'w, 's, (), With<PublishAvatarTask>>,
    inventory: Query<'w, 's, (), With<PublishInventoryTask>>,
}

impl GuardPublishTasks<'_, '_> {
    fn any_in_flight(&self) -> bool {
        !self.room.is_empty() || !self.avatar.is_empty() || !self.inventory.is_empty()
    }
}

/// Render the guard dialog and drive the pending action to a conclusion.
/// Runs in `EguiPrimaryContextPass` while [`UnsavedGuard`] exists (see
/// the registration in `crate::run`).
#[allow(clippy::too_many_arguments)]
pub fn unsaved_guard_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut guard: ResMut<UnsavedGuard>,
    records: GuardRecords,
    mut feedbacks: GuardFeedbacks,
    tasks: GuardPublishTasks,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<OauthRefreshCtx>>,
    current_room: Option<Res<CurrentRoomDid>>,
    mut next_state: ResMut<NextState<AppState>>,
    time: Res<Time>,
) {
    let owns_room = match (session.as_deref(), current_room.as_deref()) {
        (Some(session), Some(room)) => session.did == room.0,
        _ => false,
    };
    let dirty = records.compute(owns_room);

    // A publish the user fired from an editor moments before triggering
    // the action is morally the same as clicking "Publish & continue":
    // wait for it rather than racing it or double-publishing.
    if guard.phase == GuardPhase::Prompt && dirty.blocks(&guard.action) && tasks.any_in_flight() {
        guard.phase = GuardPhase::Publishing;
    }

    match guard.phase {
        GuardPhase::Publishing => {
            if tasks.any_in_flight() {
                // Still waiting on at least one poll system to drain its
                // task — render the holding state below.
            } else if !dirty.blocks(&guard.action) {
                // Every relevant publish succeeded (the polls pinned
                // stored = live) — nothing left to lose.
                proceed(&guard.action, &mut commands, &mut next_state);
                return;
            } else {
                // Drained but still dirty: at least one publish failed.
                // Fall back to the prompt with the failure surfaced.
                guard.error = Some(
                    feedbacks
                        .failure_message(&guard.action)
                        .unwrap_or_else(|| "publish did not complete".into()),
                );
                guard.phase = GuardPhase::Prompt;
            }
        }
        GuardPhase::Prompt => {
            if !dirty.blocks(&guard.action) {
                // Clean (or only irrelevant records differ): proceed
                // without ever showing the dialog. This is the everyday
                // path — callers open the guard unconditionally.
                proceed(&guard.action, &mut commands, &mut next_state);
                return;
            }
        }
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let (continue_publish, continue_discard, stay) = match guard.action {
        GuardedAction::PortalTravel { .. } => ("Publish & travel", "Discard & travel", "Stay here"),
        GuardedAction::Logout => ("Publish & log out", "Discard & log out", "Cancel"),
        GuardedAction::Quit => ("Publish & quit", "Discard & quit", "Cancel"),
    };

    egui::Modal::new(egui::Id::new("unsaved-guard")).show(ctx, |ui| {
        ui.heading("Unpublished changes");
        ui.add_space(4.0);

        let mut names: Vec<&str> = Vec::new();
        if dirty.room {
            names.push("World");
        }
        if matches!(guard.action, GuardedAction::Logout | GuardedAction::Quit) {
            if dirty.avatar {
                names.push("Avatar");
            }
            if dirty.inventory {
                names.push("Inventory");
            }
        }
        ui.label(format!(
            "You have unpublished edits to: {}.",
            names.join(", ")
        ));
        ui.label(match guard.action {
            GuardedAction::PortalTravel { .. } => "Traveling through the portal will discard them.",
            GuardedAction::Logout => "Logging out will discard them.",
            GuardedAction::Quit => "Quitting will discard them.",
        });

        if let Some(error) = &guard.error {
            ui.add_space(4.0);
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Publish failed — {error}"),
            );
        }
        ui.add_space(8.0);

        if guard.phase == GuardPhase::Publishing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Publishing…");
            });
            ui.add_space(4.0);
            // "Continue in background" (#838), not "Stay here": backing
            // out doesn't cancel the in-flight tasks — the editors' poll
            // systems land them as a normal publish — so the honest label
            // says the save keeps going.
            if ui.button("Continue in background").clicked() {
                close(&guard.action, &mut commands, &time);
            }
            return;
        }

        // Publishing needs an authenticated session; without one (which
        // should not happen in-game) only discard/stay are offered.
        let can_publish = session.is_some() && refresh_ctx.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_publish, egui::Button::new(continue_publish))
                .clicked()
                && let (Some(session), Some(refresh_ctx)) =
                    (session.as_deref(), refresh_ctx.as_deref())
            {
                guard.error = None;
                if dirty.room
                    && let Some(live) = records.live_room.as_deref()
                {
                    feedbacks.room.status = PublishStatus::Publishing;
                    let room_did = current_room
                        .as_deref()
                        .map(|d| d.0.clone())
                        .unwrap_or_default();
                    crate::ui::room::spawn_room_publish_task(
                        &mut commands,
                        session,
                        refresh_ctx,
                        live.0.clone(),
                        room_did,
                        time.elapsed_secs_f64(),
                    );
                }
                if matches!(guard.action, GuardedAction::Logout | GuardedAction::Quit) {
                    if dirty.avatar
                        && let Some(live) = records.live_avatar.as_deref()
                    {
                        feedbacks.avatar.status = PublishStatus::Publishing;
                        crate::ui::avatar::spawn_publish_avatar_task(
                            &mut commands,
                            session,
                            refresh_ctx,
                            live.0.clone(),
                            time.elapsed_secs_f64(),
                        );
                    }
                    if dirty.inventory
                        && let Some(live) = records.live_inventory.as_deref()
                    {
                        feedbacks.inventory.status = PublishStatus::Publishing;
                        crate::ui::inventory::spawn_publish_inventory_task(
                            &mut commands,
                            session,
                            refresh_ctx,
                            live.0.clone(),
                            records
                                .stored_inventory
                                .as_deref()
                                .map(|s| s.0.clone())
                                .unwrap_or_default(),
                            time.elapsed_secs_f64(),
                        );
                    }
                }
                guard.phase = GuardPhase::Publishing;
            }
            if ui.button(stay).clicked() {
                close(&guard.action, &mut commands, &time);
            }
            // Discard is the data-loss option (#838): danger-styled and
            // pushed to the far edge so it is never adjacent to the two
            // safe choices — the old row rendered three identical
            // buttons side by side.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(crate::ui::confirm::danger_button(continue_discard))
                    .clicked()
                {
                    // No revert needed: portal travel overwrites the live
                    // room record with the destination's, and logout
                    // removes every record resource outright.
                    proceed(&guard.action, &mut commands, &mut next_state);
                }
            });
        });
    });
}

/// Execute the guarded action and drop the guard.
fn proceed(action: &GuardedAction, commands: &mut Commands, next_state: &mut NextState<AppState>) {
    match action {
        GuardedAction::PortalTravel {
            target_did,
            target_pos,
        } => {
            begin_portal_travel(commands, target_did.clone(), *target_pos);
        }
        GuardedAction::Logout => {
            next_state.set(AppState::Login);
        }
        GuardedAction::Quit => {
            // `close_when_requested` is disabled so the [x] could route
            // here — exiting is now on us.
            commands.write_message(bevy::app::AppExit::Success);
        }
    }
    commands.remove_resource::<UnsavedGuard>();
}

/// Drop the guard without acting. For a declined portal travel the player
/// is still standing inside the portal collider, so a widened cooldown
/// keeps the overlap from re-opening the dialog before they can walk out.
fn close(action: &GuardedAction, commands: &mut Commands, time: &Time) {
    if matches!(action, GuardedAction::PortalTravel { .. }) {
        commands.insert_resource(PortalCooldown {
            until_secs: time.elapsed_secs_f64() + DECLINE_COOLDOWN_SECS,
        });
    }
    commands.remove_resource::<UnsavedGuard>();
}

// ---------------------------------------------------------------------
// Exit guards (#839): the guard used to cover only portals, gateways and
// logout — closing the native window or the browser tab bypassed it
// entirely and took the unsaved edits down with the process.
// ---------------------------------------------------------------------

/// Native: intercept the window's close button. The `WindowPlugin` is built with
/// `close_when_requested: false`, so nothing closes until this system
/// decides: clean records exit immediately; dirty ones raise the same
/// guard dialog portals and logout use, as [`GuardedAction::Quit`].
/// Runs in every `AppState` — outside `InGame` the record resources are
/// absent, the dirty set is empty, and the close is unprompted.
#[cfg(not(target_arch = "wasm32"))]
pub fn intercept_window_close(
    mut close_requested: MessageReader<bevy::window::WindowCloseRequested>,
    records: GuardRecords,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
    guard: Option<Res<UnsavedGuard>>,
    mut commands: Commands,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if close_requested.is_empty() {
        return;
    }
    close_requested.clear();
    // A guard dialog is already up (possibly mid-publish) — a second [x]
    // must not bypass it.
    if guard.is_some() {
        return;
    }
    let owns_room = matches!(
        (session.as_deref(), current_room.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    if records.compute(owns_room).blocks(&GuardedAction::Quit) {
        commands.insert_resource(UnsavedGuard::new(GuardedAction::Quit));
    } else {
        exit.write(bevy::app::AppExit::Success);
    }
}

/// wasm: the live "would closing this tab lose work?" bit for the
/// `beforeunload` listener. A JS event handler can't query the ECS, so
/// [`sync_beforeunload_dirty`] mirrors the dirty state here and the
/// listener just reads it.
#[cfg(target_arch = "wasm32")]
static BEFOREUNLOAD_DIRTY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// How often the wasm dirty mirror recomputes. The diff serializes all
/// three records, so it is throttled rather than per-frame; a one-second
/// stale window on a browser-close prompt is imperceptible.
#[cfg(target_arch = "wasm32")]
const BEFOREUNLOAD_SYNC_INTERVAL_SECS: f64 = 1.0;

/// wasm: throttled mirror of the guard's derived dirty set into
/// [`BEFOREUNLOAD_DIRTY`]. Runs in every `AppState`; with the record
/// resources absent (login screen) the flag settles to `false` and the
/// tab closes unprompted.
#[cfg(target_arch = "wasm32")]
pub fn sync_beforeunload_dirty(
    records: GuardRecords,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
    time: Res<Time>,
    mut next_check: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_check {
        return;
    }
    *next_check = now + BEFOREUNLOAD_SYNC_INTERVAL_SECS;
    let owns_room = matches!(
        (session.as_deref(), current_room.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    let dirty = records.compute(owns_room).blocks(&GuardedAction::Quit);
    BEFOREUNLOAD_DIRTY.store(dirty, std::sync::atomic::Ordering::Relaxed);
}

/// wasm: install the `beforeunload` listener once at startup. While the
/// mirrored dirty flag is set, closing/reloading the tab raises the
/// browser's own leave-site confirm; while clean it does nothing at all
/// (no `preventDefault`, no return value — an unconditional handler
/// would nag on every navigation). Leaked via `Closure::forget`: it must
/// live for the whole page lifetime anyway.
#[cfg(target_arch = "wasm32")]
pub fn install_beforeunload_guard() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(window) = web_sys::window() else {
        return;
    };
    let closure = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
        move |event: web_sys::BeforeUnloadEvent| {
            if BEFOREUNLOAD_DIRTY.load(std::sync::atomic::Ordering::Relaxed) {
                // Modern browsers ignore the string and show their own
                // wording; preventDefault + a non-empty return value is
                // the cross-browser way to request the prompt.
                event.prevent_default();
                event.set_return_value("You have unsaved edits.");
            }
        },
    );
    if let Err(e) =
        window.add_event_listener_with_callback("beforeunload", closure.as_ref().unchecked_ref())
    {
        warn!("failed to install beforeunload guard: {e:?}");
    }
    closure.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty(room: bool, avatar: bool, inventory: bool) -> DirtyRecords {
        DirtyRecords {
            room,
            avatar,
            inventory,
        }
    }

    fn travel() -> GuardedAction {
        GuardedAction::PortalTravel {
            target_did: "did:plc:example".into(),
            target_pos: Some(Vec3::ZERO),
        }
    }

    #[test]
    fn portal_travel_only_blocks_on_room_dirt() {
        assert!(dirty(true, false, false).blocks(&travel()));
        // Avatar and inventory survive a portal hop — they must not gate it.
        assert!(!dirty(false, true, true).blocks(&travel()));
        assert!(!dirty(false, false, false).blocks(&travel()));
    }

    #[test]
    fn logout_blocks_on_any_dirt() {
        assert!(dirty(true, false, false).blocks(&GuardedAction::Logout));
        assert!(dirty(false, true, false).blocks(&GuardedAction::Logout));
        assert!(dirty(false, false, true).blocks(&GuardedAction::Logout));
        assert!(!dirty(false, false, false).blocks(&GuardedAction::Logout));
    }
}
