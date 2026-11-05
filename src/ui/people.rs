//! Room roster window. Lists the signed-in user plus every remote peer
//! currently in the room, with a per-peer Mute toggle. The mute flag writes
//! straight into `RemotePeer.muted`; audio-mix / visibility code keys off the
//! same component. Diagnostics still renders its own copy of the roster
//! (with DIDs) - this window is the user-facing social view, Diagnostics is
//! the debug view.
//!
//! Drag-to-gift: peer rows double as drop targets for the inventory /
//! world-editor drag. While a drag of any kind is armed, hovering a peer row
//! writes a [`crate::ui::inventory::PeerDropTarget`] into
//! [`crate::ui::inventory::PendingGeneratorDrop`]; releasing there routes
//! the drop into an [`crate::protocol::OverlandsMessage::ItemOffer`]
//! instead of a ground placement. See [`crate::ui::inventory::handle_generator_drop`].
//!
//! A whole avatar or world (#1844) is offered the same way, to a peer whose
//! build can take one ([`gift_block_reason`] reads the drag's kind against
//! the peer's announced protocol); an older or still-unknown build's row
//! refuses it and says why.
//!
//! Incoming offer modal: when [`crate::state::IncomingOfferDialog`] is set,
//! [`incoming_offer_ui`] renders the Accept / Decline / Mute & Decline
//! prompt. Exactly one dialog is ever active - concurrent offers are
//! auto-declined with "busy" at the network layer, see
//! [`crate::network`]. It names what is offered and how large: a thing's
//! kind, an avatar's kind - the one you have on stays on - or a world's
//! kind, fetched from the giver's saved copy on Accept with nothing in your
//! own world changed. A thing or an avatar lands at the click; a world when
//! its parts arrive ([`crate::ui::inventory::gift_world`]), and Accept for
//! one is disabled, its reason painted, for a sign-in that cannot save it.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::BskyProfileCache;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::network::presence::{PeerLabel, PeerResolve, peer_status, set_peer_mute};
use crate::pds::InventoryRecord;
use crate::protocol::{GiftItem, GiftKind, OverlandsMessage};
use crate::state::{
    IncomingOfferDialog, LiveInventoryRecord, PublishFeedback, RemotePeer, SocialResonance,
};
use crate::ui::avatar::draw_avatar_icon;
use crate::ui::chat::AVATAR_ICON_PX;
use crate::ui::inventory::{PeerDropTarget, PendingGeneratorDrop};

/// Where a peer's row sits in the roster (#844, corrected by #1226 f325).
///
/// Pure, because the sort is a claim about identity and the review found it
/// making a false one: the key was `handle.unwrap_or("~")`, so every peer
/// whose handle had not resolved shared a single key. Two strangers were
/// adjacent, tied and interchangeable - and a bare query iteration follows
/// archetype order, so a tie is resolved by whatever the last component
/// insert did. A row that can swap places between two frames in which
/// nothing about anybody changed is a row you cannot aim a durable,
/// account-scoped mute at.
///
/// Three tiers, most-deliberate first: mutuals, then the label ladder the
/// row actually renders ([`PeerLabel::sort_key`]), then the peer id - which
/// is unique, stable for the connection's lifetime, and exists precisely so
/// the previous tiers never have to tie.
fn roster_sort_key(
    peer: &RemotePeer,
    resonance: Option<&SocialResonance>,
) -> (bool, (u8, String), String) {
    (
        !matches!(resonance, Some(SocialResonance::Mutual)),
        PeerLabel::new(peer.handle.as_deref(), peer.did.as_deref()).sort_key(),
        peer.peer_id.to_string(),
    )
}

/// Why a peer row cannot accept a dropped gift, or `None` when it can
/// (#1220 f330).
///
/// Pure, and the ONE definition: the same answer drives the row's drag
/// highlight, its hover text, and the toast `handle_generator_drop` raises
/// on a release. Before this the condition was an anonymous boolean that
/// silently suppressed the highlight, the target and the explanation
/// together - so the user's release did nothing and said nothing.
///
/// Order matters: a muted peer is muted whether or not they have identified,
/// and saying "still identifying" about somebody you deliberately blocked
/// would be the wrong sentence.
///
/// A whole avatar or world (`kind`, #1844) goes only to a peer whose `Hello`
/// said [`crate::protocol::WHOLE_GIFTS_SINCE`] or later, read off its
/// compatibility as of `now`. A thing goes to any build, as it always has.
pub fn gift_block_reason(
    peer: &RemotePeer,
    link_is_up: bool,
    kind: GiftKind,
    now: f64,
) -> Option<&'static str> {
    if !link_is_up {
        // A gift sent while our link is down spends an inventory item on an
        // offer that can never be answered, and then blames the recipient
        // three minutes later (#1213 f404).
        return Some("You're not connected right now - the offer wouldn't reach them.");
    }
    if peer.muted {
        return Some("You've muted this person. Unmute them to send a gift.");
    }
    if peer.did.is_none() {
        return Some(
            "Still identifying - a gift is addressed to an account, and theirs hasn't \
             arrived yet. Try again in a moment.",
        );
    }
    if kind.is_whole() {
        use crate::state::PeerCompatibility;
        match peer.compatibility(now) {
            PeerCompatibility::Pending => {
                return Some(
                    "Still finding out which version of the game they run - try again in a \
                     moment.",
                );
            }
            PeerCompatibility::Unannounced => return Some(OLDER_BUILD),
            PeerCompatibility::Mismatched(theirs)
                if theirs < crate::protocol::WHOLE_GIFTS_SINCE =>
            {
                return Some(OLDER_BUILD);
            }
            PeerCompatibility::Compatible | PeerCompatibility::Mismatched(_) => {}
        }
    }
    None
}

/// Why a whole avatar or world cannot go to a peer on an older build (#1844).
const OLDER_BUILD: &str =
    "Their game is an older version that cannot take a whole avatar or world.";

/// Why the Visit button is disabled, or `None` when it works (#1220 f302).
///
/// Five states used to make the button VANISH, and an absent control cannot
/// carry a tooltip. Three of the five are visible elsewhere (the guard is a
/// blocking modal, a travel paints a banner, a mute ticks its own checkbox),
/// but "you are already in their world" had no cue anywhere - and a control
/// that comes and goes reads as an unreliable feature rather than a
/// temporarily unavailable one.
pub fn visit_block_reason(
    peer: &RemotePeer,
    already_here: bool,
    traveling: bool,
    guarded: bool,
) -> Option<&'static str> {
    if peer.did.is_none() {
        return Some("Still identifying - their world is addressed by account.");
    }
    if already_here {
        return Some("You're already in their world.");
    }
    if peer.muted {
        return Some("You've muted this person.");
    }
    if traveling {
        return Some("You're already travelling somewhere.");
    }
    if guarded {
        return Some("Finish or discard your unsaved edits first.");
    }
    None
}

/// Everything the roster reads that is not the roster itself.
///
/// Bundled because `people_ui` sat at Bevy's 16-parameter ceiling (#1213
/// took the last slot) and this tranche needs to add to it: the `⋯` menu's
/// clipboard (#1223 f291) had nowhere to go. Bundling first, then adding, is
/// the order - an over-ceiling system fails at app build with a trait error
/// that names none of this.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RosterDeps<'w> {
    time: Res<'w, Time>,
    session_log: ResMut<'w, crate::diagnostics::SessionLog>,
    pending_offers: Res<'w, crate::state::PendingOutgoingOffers>,
    muted_dids: ResMut<'w, crate::state::MutedDids>,
    current_room: Option<Res<'w, crate::state::CurrentRoomDid>>,
    traveling: Option<Res<'w, crate::state::TravelingTo>>,
    guard: Option<Res<'w, crate::ui::unsaved_guard::UnsavedGuard>>,
    link: Res<'w, crate::network::LinkState>,
    /// The `…` menu's Copy DID (#1223 f291). Interior-mutable, so a `Res`.
    clipboard: Res<'w, crate::boot_params::ClipboardQueue>,
    /// The link between this list and the bodies in the room (#1226 f325):
    /// this window writes `row`, the nametag surface writes `tag`, and each
    /// reads the other's half.
    focus: ResMut<'w, crate::ui::nametag::PeerFocus>,
}

#[allow(clippy::too_many_arguments)]
pub fn people_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    session: Option<Res<AtprotoSession>>,
    // `PeerResolve` rides the peer query rather than arriving as its own
    // parameter, for the same ceiling reason `RosterDeps` exists.
    mut peers: Query<(
        Entity,
        &mut RemotePeer,
        Option<&SocialResonance>,
        &PeerResolve,
    )>,
    profile_cache: Res<BskyProfileCache>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    mut commands: Commands,
    mut deps: RosterDeps,
) {
    let now = deps.time.elapsed_secs_f64();

    // Drag-to-gift hover snapshot lives in `pending_drop.peer_target`. We
    // rebuild it from scratch each frame because a peer that was hovered
    // last frame may no longer be under the cursor this frame; leaving the
    // stale value in place would let the drop handler target a peer the
    // user had already moved away from. Only overwrite when a drag is
    // armed, so the resource is untouched outside a drag (and a later
    // non-drag interaction can't accidentally poke it).
    let drag_active = pending_drop.generator_name.is_some();
    if drag_active {
        pending_drop.peer_target = None;
    }

    let ctx = contexts.ctx_mut().unwrap();
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::People, ctx);
    // Guarded-dirty (#879): `.open(&mut panels.people)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce - local copy in, write back only on close.
    let mut open = panels.people;
    let response = egui::Window::new("People")
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(chrome.available_rect(ctx))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            let peer_count = peers.iter().count();
            let total = peer_count + session.is_some() as usize;
            // "In room (N)" is a claim about the ROOM, and a client whose
            // link is down is in no position to make one (#1213 f395) -
            // that copy is exactly what made an outage read as an empty
            // product. The wording lives on `LinkPhase` so this window, the
            // toolbar chip and the chat note cannot drift.
            let phase = deps.link.phase();
            let header = phase.roster_header(total);
            if phase.is_up() {
                ui.label(header);
            } else {
                ui.colored_label(crate::ui::theme::current(ui.ctx()).status.warn, header)
                    .on_hover_text(phase.sentence());
            }
            ui.separator();

            // Rebuilt from scratch each frame, like `pending_drop.peer_target`
            // above: a hover is a fact about THIS frame, and a stale one aims
            // a highlight at a body the user has already moved away from.
            let mut row_focus: Option<Entity> = None;
            egui::ScrollArea::vertical()
                .auto_shrink([true, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    // Self entry at the top. Same blue dot the chat uses
                    // for the local author tag, so the visual "you" cue
                    // carries across both windows. No Mute button on self.
                    if let Some(s) = session.as_deref() {
                        // Same info-blue the chat uses for the local
                        // author tag (#856) - the "you" cue carries across.
                        let self_color = crate::ui::theme::current(ui.ctx()).status.info;
                        ui.horizontal(|ui| {
                            crate::ui::affordances::status_dot(ui, self_color);
                            draw_avatar_icon(
                                ui,
                                Some(s.did.as_str()),
                                Some(s.handle.as_str()),
                                &profile_cache,
                                AVATAR_ICON_PX,
                            );
                            ui.monospace(format!("@{} (you)", s.handle));
                        });
                    }

                    // Remote peers. A peer whose profile has not resolved
                    // shows the head of their authenticated DID, and one
                    // that has not identified at all shows "A traveler" -
                    // the same ladder the presence lines use (#1218), in
                    // place of the bespoke "identifying…" this row invented.
                    //
                    // Sorted (#844): bare query iteration follows archetype
                    // order, so rows JUMPED when `SocialResonance` resolved
                    // (a component insert moves the entity). Mutuals first,
                    // then case-insensitive handle - the same deliberate
                    // order the gateway picker uses; a stable list also
                    // de-risks drag-to-gift aim.
                    //
                    // Sorted on the LABEL ladder, not on the handle (#1226
                    // f325): `handle.unwrap_or("~")` gave every handle-less
                    // peer one key, so two strangers were adjacent and
                    // interchangeable - and, tied, they took whatever order
                    // the sort happened to leave them in, under a pointer
                    // aiming a durable mute. `peer_id` breaks the last tie so
                    // the list cannot reshuffle between two frames in which
                    // nothing about anybody changed.
                    let mut rows: Vec<_> = peers.iter_mut().collect();
                    rows.sort_by_key(|(_, peer, resonance, _)| roster_sort_key(peer, *resonance));
                    for (entity, mut peer, resonance, resolve) in rows {
                        // The ONE ladder (#1218 f300/f338): a verified handle,
                        // else the authenticated DID's head, else "A traveler".
                        // "identifying…" was a fourth name for a peer who
                        // already had a perfectly good identifier.
                        let label = PeerLabel::new(peer.handle.as_deref(), peer.did.as_deref());
                        let th = crate::ui::theme::current(ui.ctx());
                        let dot_color = if peer.muted { th.text_faint } else { th.status.ok };
                        let mut muted = peer.muted;

                        // Render the row inside an egui Response we can
                        // interrogate for hover state so we can highlight
                        // valid drop targets while a drag is armed. Peers
                        // without a resolved DID cannot receive offers
                        // (the recipient authenticates by DID), so their
                        // row stays inert for the drag.
                        // A drop onto a peer we cannot reach spends an
                        // inventory item on an offer that can never be
                        // answered, and then blames the recipient three
                        // minutes later (#1213 f404) - so the row is inert
                        // whenever the link is not up, alongside the mute
                        // and DID-resolution gates it already had.
                        // Why this row cannot take a gift, if it cannot
                        // (#1220 f330). An ineligible row used to give no
                        // highlight during the drag AND no explanation on
                        // release - `handle_generator_drop` found no target
                        // and fell through to the silent egui-cancel written
                        // for releases over the Inventory window. Silence on
                        // release is indistinguishable from a broken
                        // feature, and the most common ineligible state is a
                        // normal transient: a peer's DID resolves a beat
                        // after they appear, which is exactly when a new
                        // user tries their first gift.
                        let blocked =
                            gift_block_reason(&peer, deps.link.is_up(), pending_drop.kind, now);
                        let row = ui.horizontal(|ui| {
                            crate::ui::affordances::status_dot(ui, dot_color);
                            draw_avatar_icon(
                                ui,
                                peer.did.as_deref(),
                                peer.handle.as_deref(),
                                &profile_cache,
                                AVATAR_ICON_PX,
                            );
                            // A peer the local user mutually follows gets
                            // the shared warm-gold name + a ★ so the cue
                            // also survives a colour-blind / greyscale
                            // viewer. `SocialResonance` is absent until the
                            // async getRelationships query lands; treat
                            // missing / Unknown / None as "not a mutual".
                            if matches!(resonance, Some(SocialResonance::Mutual)) {
                                // Accent, not gold (#856): the old
                                // (240,190,70) star sat in the warn-amber
                                // family - a friend must not read as a
                                // caution. Brand highlight = accent.
                                ui.colored_label(
                                    crate::ui::theme::current(ui.ctx()).accent,
                                    egui::RichText::new(format!("★ {}", label.addressed()))
                                        .monospace(),
                                )
                                .on_hover_text("You and this peer follow each other");
                            } else {
                                ui.monospace(label.addressed());
                                // "Couldn't ask" is not "no" (#1218 f297).
                                // The ★ is the only trust signal this UI
                                // carries and it used to fail closed: a
                                // timed-out relationship query rendered
                                // exactly like a stranger, so a friend
                                // quietly became someone to be careful with.
                                if matches!(resonance, Some(SocialResonance::Failed)) {
                                    ui.colored_label(
                                        crate::ui::theme::current(ui.ctx()).text_weak,
                                        egui::RichText::new("?").monospace(),
                                    )
                                    .on_hover_text(
                                        "Couldn't check whether you follow each other. \
                                         Trying again shortly.",
                                    );
                                }
                            }
                            // Outgoing-gift badge (#843): while an offer to
                            // this peer awaits their answer, say so on the
                            // row - the sender used to have no trace at all.
                            let offers_pending = peer
                                .did
                                .as_deref()
                                .map(|did| {
                                    deps.pending_offers
                                        .by_id
                                        .values()
                                        .filter(|o| o.target_did == did)
                                        .count()
                                })
                                .unwrap_or(0);
                            if offers_pending > 0 {
                                let text = if offers_pending == 1 {
                                    "🎁 offer pending".to_owned()
                                } else {
                                    format!("🎁 {offers_pending} offers pending")
                                };
                                ui.label(
                                    egui::RichText::new(text).small().color(crate::ui::theme::current(ui.ctx()).text_weak),
                                )
                                .on_hover_text("Waiting for this peer to accept or decline");
                            }
                            // While a gift drag is armed, an ineligible row
                            // says so on the row itself rather than waiting
                            // for a hover (#1220 f330) - the user is
                            // mid-drag, hunting for a target, and is not
                            // going to stop and hover. Only during a drag,
                            // so an idle roster is not littered with
                            // sentences about a gesture nobody is making.
                            if drag_active
                                && let Some(reason) = blocked
                            {
                                ui.colored_label(
                                    crate::ui::theme::current(ui.ctx()).text_weak,
                                    egui::RichText::new("can't receive").small(),
                                )
                                .on_hover_text(reason);
                            }
                            // How this person is loading, in ONE chip
                            // (#1217/#1218). Six separate failures - no
                            // identity, a failed avatar fetch, a failed
                            // profile fetch, an incomplete outfit, a body
                            // still building - each used to reach the screen
                            // nowhere at all, and the temptation was to give
                            // each its own badge. `peer_status` picks the
                            // worst one; a row that can carry four warnings
                            // at once is a row nobody reads.
                            // Not for a muted peer: how well somebody you
                            // have blocked is loading is not news, and the
                            // faint dot and ticked checkbox already say what
                            // their row is (#1219).
                            if let Some(status) =
                                peer_status(peer.did.is_some(), resolve).filter(|_| !peer.muted)
                            {
                                let th = crate::ui::theme::current(ui.ctx());
                                let color = if status.is_warning() {
                                    th.status.warn
                                } else {
                                    th.text_weak
                                };
                                ui.colored_label(
                                    color,
                                    egui::RichText::new(status.chip()).small(),
                                )
                                .on_hover_text(status.hover());
                            }
                            // Wire-compatibility chip (#1121). Until this
                            // existed, a gift to an incompatible peer looked
                            // exactly like a gift to a peer who had not
                            // answered yet - the offer badge above sat there
                            // forever and neither end could learn why. The
                            // chip does not make the two builds compatible;
                            // it makes the pending badge legible.
                            match peer.compatibility(now) {
                                crate::state::PeerCompatibility::Mismatched(theirs) => {
                                    let ours = crate::protocol::PROTOCOL_VERSION;
                                    let detail = peer
                                        .build
                                        .as_ref()
                                        .map(|b| b.build.as_str())
                                        .unwrap_or("unknown");
                                    ui.colored_label(
                                        crate::ui::theme::current(ui.ctx()).status.warn,
                                        egui::RichText::new("⚠ build").small(),
                                    )
                                    .on_hover_text(format!(
                                        "This peer speaks wire protocol {theirs} ({detail}); \
                                         we speak {ours}. Chat and movement may work, but \
                                         gifts and room updates between you can fail to \
                                         decode and are dropped without notice."
                                    ));
                                }
                                crate::state::PeerCompatibility::Unannounced => {
                                    ui.colored_label(
                                        crate::ui::theme::current(ui.ctx()).status.warn,
                                        egui::RichText::new("⚠ build").small(),
                                    )
                                    .on_hover_text(
                                        "This peer never announced a wire protocol, so it is \
                                         running a build from before the handshake existed. \
                                         Gifts and room updates between you can fail to decode \
                                         and are dropped without notice.",
                                    );
                                }
                                crate::state::PeerCompatibility::Compatible
                                | crate::state::PeerCompatibility::Pending => {}
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // The only stable identifier a person has
                                    // (#1223 f291). Until this menu, the app's
                                    // social surface carried none: the sole
                                    // place to copy the DID of somebody
                                    // standing next to you was a collapsing
                                    // fold labelled "Peers (debug)". Copying
                                    // it is what turns an unreportable
                                    // stranger into somebody the user can
                                    // block at the ATProto layer, warn a
                                    // friend about, or report off-platform -
                                    // the product ships no report path of its
                                    // own.
                                    ui.menu_button("…", |ui| {
                                        match peer.did.as_deref() {
                                            Some(did) => {
                                                if ui.button("Copy account id").clicked() {
                                                    deps.clipboard.copy(
                                                        did,
                                                        &format!("Copied: {did}"),
                                                    );
                                                    ui.close();
                                                }
                                                if ui
                                                    .button("Open Bluesky profile")
                                                    .on_hover_text(
                                                        "Opens bsky.app in your browser, \
                                                         where you can block or report \
                                                         this account.",
                                                    )
                                                    .clicked()
                                                {
                                                    crate::ui::affordances::open_url_in_browser(
                                                        &crate::config::network::bsky_profile_url(
                                                            did,
                                                        ),
                                                    );
                                                    ui.close();
                                                }
                                            }
                                            None => {
                                                ui.label(
                                                    egui::RichText::new(
                                                        "No account id yet - this peer \
                                                         hasn't identified itself.",
                                                    )
                                                    .small()
                                                    .color(
                                                        crate::ui::theme::current(ui.ctx())
                                                            .text_weak,
                                                    ),
                                                );
                                            }
                                        }
                                    });
                                    // A mute is remembered against an ACCOUNT
                                    // (#844), so a peer with no authenticated
                                    // DID can only be muted for as long as
                                    // this entity lives - the tooltip's
                                    // "Persists across sessions" was simply
                                    // false for them, and the checkbox did
                                    // half its job in silence (#1218 f290).
                                    // Now it refuses, and says why.
                                    let identified = peer.did.is_some();
                                    ui.add_enabled_ui(identified, |ui| {
                                        ui.checkbox(&mut muted, "Mute").on_hover_text(
                                            "Hides their avatar, chat, audio and gift \
                                             offers. Persists across sessions.",
                                        );
                                    })
                                    .response
                                    .on_disabled_hover_text(
                                        "A mute is remembered against an account, and this \
                                         peer hasn't identified itself yet. Their messages \
                                         are already being dropped until it does.",
                                    );
                                    // "Meet someone → visit their world"
                                    // finally has a UI path (#845). Routed
                                    // through the unsaved-edits guard exactly
                                    // like gateway travel; `target_pos: None`
                                    // arrives at their default landing.
                                    //
                                    // Rendered ALWAYS and disabled with a
                                    // reason (#1220 f302): an absent control
                                    // cannot carry a tooltip, and a button
                                    // that comes and goes teaches the user
                                    // the feature is unreliable rather than
                                    // temporarily unavailable. The reason
                                    // matters most for "you are already in
                                    // their world", which is the one of the
                                    // five gating states with no cue
                                    // anywhere else on screen.
                                    let already_here = peer.did.as_deref().is_some_and(|did| {
                                        deps.current_room
                                            .as_deref()
                                            .is_some_and(|room| room.0 == did)
                                    });
                                    let visit_blocked = visit_block_reason(
                                        &peer,
                                        already_here,
                                        deps.traveling.is_some(),
                                        deps.guard.is_some(),
                                    );
                                    let visit = ui
                                        .add_enabled(
                                            visit_blocked.is_none(),
                                            egui::Button::small(egui::Button::new("Visit")),
                                        )
                                        .on_hover_text(format!(
                                            "Travel to {}'s world",
                                            label.addressed()
                                        ));
                                    let visit = match visit_blocked {
                                        Some(reason) => visit.on_disabled_hover_text(reason),
                                        None => visit,
                                    };
                                    if visit.clicked()
                                        && let Some(did) = peer.did.as_deref()
                                    {
                                        commands.insert_resource(
                                            crate::ui::unsaved_guard::UnsavedGuard::new(
                                                crate::ui::unsaved_guard::GuardedAction::PortalTravel {
                                                    via: crate::ui::unsaved_guard::TravelVia::Visit,
                                                    target_did: did.to_owned(),
                                                    // The row's own name,
                                                    // carried (#1231 f27):
                                                    // the overlay used to
                                                    // fall back to a DID
                                                    // head one click after
                                                    // this row said
                                                    // "@alice".
                                                    target_label: Some(label.addressed()),
                                                    target_pos: None,
                                                },
                                            ),
                                        );
                                    }
                                },
                            );
                        });
                        let row_rect = row.response.rect;
                        // The world half of the link (#1226 f325): the
                        // pointer is over this person's nametag out in the
                        // room, so say which row they are. Painted on top
                        // with the same translucent accent the gift-drag
                        // highlight uses, so the two cues read as one idiom.
                        if deps.focus.tag == Some(entity) {
                            let a = crate::ui::theme::current(ui.ctx()).accent;
                            ui.painter().rect_filled(
                                row_rect,
                                4.0,
                                egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 40),
                            );
                        }
                        // ...and the roster half: hovering a row is a
                        // deliberate "which one is this?", answered by a
                        // wire box around that body and a brighter tag over
                        // it (`ui::nametag::draw_focused_peer_highlight`).
                        // Recorded for ANY hover, not just a drag - aiming a
                        // mute is the case that matters most and involves no
                        // drag at all.
                        if ui.rect_contains_pointer(row_rect) {
                            row_focus = Some(entity);
                        }
                        let hovered = drag_active && ui.rect_contains_pointer(row_rect);
                        if hovered && blocked.is_none() {
                            // Soft highlight so the user has visual
                            // feedback that a release here is a gift and
                            // not a mis-click. Painted on the foreground
                            // layer so it overlays the default row bg
                            // without disturbing text layout.
                            ui.painter().rect_filled(
                                row_rect,
                                4.0,
                                {
                                    let a = crate::ui::theme::current(ui.ctx()).accent;
                                    egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 40)
                                },
                            );
                            if let Some(did) = peer.did.clone() {
                                pending_drop.peer_target = Some(PeerDropTarget {
                                    peer_id: peer.peer_id,
                                    did,
                                    label: label.addressed(),
                                    blocked: None,
                                });
                            }
                        } else if let Some(reason) = hovered.then_some(blocked).flatten() {
                            // Recorded WITH its reason so the drop handler
                            // can say something instead of falling through
                            // to the silent cancel (#1220 f330). The DID may
                            // be absent - that is one of the reasons - so
                            // the target carries an empty one; nothing on
                            // this path sends a message.
                            pending_drop.peer_target = Some(PeerDropTarget {
                                peer_id: peer.peer_id,
                                did: peer.did.clone().unwrap_or_default(),
                                label: label.addressed(),
                                blocked: Some(reason),
                            });
                        }

                        // One funnel for every mute write (#1219): it owns
                        // the change guard that keeps `Changed<RemotePeer>`
                        // meaningful, the durable DID-keyed mirror (#844) and
                        // the session-log line, so this control and the offer
                        // dialog's "Mute & Decline" cannot drift apart.
                        let (peer_id, did) = (peer.peer_id, peer.did.clone());
                        set_peer_mute(
                            Some(&mut peer),
                            did.as_deref(),
                            muted,
                            &mut deps.muted_dids,
                            &mut deps.session_log,
                            Some(peer_id),
                            now,
                        );
                    }

                    if peer_count == 0 && session.is_none() {
                        ui.colored_label(crate::ui::theme::current(ui.ctx()).text_weak, "(empty)");
                    } else if peer_count == 0 {
                        // "(no other peers)" is the same unearned claim as
                        // the header; `roster_empty_note` says who is
                        // speaking instead (#1213 f395).
                        ui.colored_label(
                            crate::ui::theme::current(ui.ctx()).text_weak,
                            phase.roster_empty_note(),
                        );
                    }
                });
            row_focus
        });
    // A closed or collapsed window hovers nothing, and `show` hands back
    // `None` for the body in both cases - so the link clears itself without
    // this needing to know which of the two happened.
    let row_focus = response.as_ref().and_then(|r| r.inner.flatten());
    crate::ui::nametag::PeerFocus::set_row(&mut deps.focus, row_focus);
    if let Some(response) = response {
        chrome.remember(crate::ui::layout::UiWindow::People, response.response.rect);
    }
    if panels.people && !open {
        panels.people = false;
    }
}

/// Present when something other than the offer dialog answers incoming
/// offers (#1423): the agent client, whose window no person sees. An offer
/// still arrives, waits and expires exactly as ever - the busy gate, the
/// answer window's timeout and a muted sender's dismissal all run on
/// [`IncomingOfferDialog`] - but [`incoming_offer_ui`] draws nothing: a
/// modal nobody could answer held every movement key for as long as the
/// offer waited.
#[derive(Resource)]
pub struct OffersAnsweredElsewhere;

/// Everything answering an incoming offer touches (#1423). The dialog's
/// buttons and the agent client both answer through [`Self::answer`], so
/// an offer is answered one way whoever answers it.
#[derive(SystemParam)]
pub struct OfferAnswering<'w, 's> {
    commands: Commands<'w, 's>,
    live_inventory: Option<ResMut<'w, LiveInventoryRecord>>,
    stored_inventory: Option<Res<'w, crate::state::StoredInventoryRecord>>,
    session: Option<Res<'w, AtprotoSession>>,
    refresh_ctx: Option<Res<'w, crate::oauth::OauthRefreshCtx>>,
    peers: Query<'w, 's, &'static mut RemotePeer>,
    writer: MessageWriter<'w, Broadcast<OverlandsMessage>>,
    session_log: ResMut<'w, SessionLog>,
    inventory_feedback: ResMut<'w, PublishFeedback<InventoryRecord>>,
    // The save on accept must not write over a stash that was never read
    // (#1199); see the accept arm.
    inventory_recovery: Option<Res<'w, crate::state::InventoryRecordRecovery>>,
    time: Res<'w, Time>,
    metrics: ResMut<'w, crate::diagnostics::MetricsRegistry>,
    busy_declines: ResMut<'w, crate::state::BusyAutoDeclines>,
    toasts: ResMut<'w, crate::notify::Toasts>,
    muted_dids: ResMut<'w, crate::state::MutedDids>,
    /// How an accepted world's parts are fetched (#1844): the network's,
    /// unless a test stands one in.
    gift_fetch: Option<Res<'w, crate::ui::inventory::GiftWorldFetch>>,
}

/// How an incoming offer is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OfferAnswer {
    /// Take the gift into the live inventory. With `publish`, save the
    /// stash as saved plus the gift at once, as the dialog's Accept always
    /// does; the agent client saves only when it was allowed to.
    Accept {
        publish: bool,
    },
    Decline,
    MuteAndDecline,
}

/// Where an accepted gift landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedGift {
    /// Its name in the inventory - the offered name, or that name with a
    /// suffix when the stash already held one.
    pub key: String,
    /// Whether a save of the inventory was started for it.
    pub saving: bool,
}

/// Renders the incoming-offer modal when [`IncomingOfferDialog`] is set
/// and drives the Accept / Decline / Mute & Decline actions. On accept,
/// the item is copied into the owner's live inventory under a
/// collision-safe key (see [`crate::ui::inventory::accept_gift`]) and a publish
/// task is spawned immediately so the new item is on the PDS before the
/// user closes the window - the user explicitly opted into "auto-publish
/// on accept" for less-likely-to-lose-items behaviour. A world (#1844)
/// lands when its parts have been fetched from the giver's saved copy. Not
/// run where [`OffersAnsweredElsewhere`] stands.
pub fn incoming_offer_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    dialog: Option<Res<IncomingOfferDialog>>,
    mut answering: OfferAnswering,
    mut offer_size: Local<Option<(u64, Option<usize>)>>,
) {
    let Some(dialog) = dialog else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut action: Option<OfferAction> = None;
    // A true `egui::Modal`, matching the unsaved-edits guard - the
    // app's one modality pattern (#834). It always paints topmost and
    // blocks background input, so it can never end up buried under the
    // (previously also center-anchored) Controls sheet with its
    // buttons unreachable.
    crate::ui::confirm::note_modal_open(ctx);
    let modal = egui::Modal::new(egui::Id::new("incoming-item-offer")).show(ctx, |ui| {
        ui.heading("Incoming item offer");
        ui.add_space(4.0);
        // Who is asking is the most consequential thing in this dialog and
        // the user has a countdown to judge it (#1218 f299). When the handle
        // has not resolved, SAY that - the old copy put the raw DID inside an
        // `@`-prefixed sentence, which reads as a name and is not one.
        if dialog.sender_label.is_named() {
            ui.label(format!(
                "{} wants to gift you \"{}\".",
                dialog.sender_label.addressed(),
                dialog.item_name
            ));
        } else {
            ui.label(format!(
                "Someone whose name hasn't loaded yet wants to gift you \"{}\".",
                dialog.item_name
            ));
        }
        // Labelled, so the string below reads as an identifier rather than as
        // a second attempt at a name.
        ui.monospace(
            egui::RichText::new(format!("account id: {}", dialog.sender_did))
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
        // What's actually being offered (#843): kind + rough size. The item
        // arrives decoded + sanitized before the dialog opens; the size is
        // measured once per offer (cached by offer_id - serializing per
        // frame would be wasted work). A world's is its head's word for
        // the saved world (#1844), which nothing here fetches.
        let bytes = match *offer_size {
            Some((id, bytes)) if id == dialog.offer_id => bytes,
            _ => {
                let bytes = offer_bytes(&dialog.item);
                *offer_size = Some((dialog.offer_id, bytes));
                bytes
            }
        };
        let size_text = bytes
            .map(crate::pds::record_size::human_bytes)
            .unwrap_or_else(|| "size unknown".to_owned());
        ui.label(
            egui::RichText::new(format!("{} · {size_text}", offer_kind_label(&dialog.item)))
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
        if let Some(note) = offer_note(&dialog.item, &dialog.sender_label) {
            ui.add(egui::Label::new(egui::RichText::new(note).small()).wrap());
        }
        ui.separator();
        // Lifted out of the `if let Some(live)` (#1220 f302): a missing
        // record must produce visible text too, not a silent grey Accept -
        // which `answer_row` paints below, beside the button it disables.
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        if let Some(live) = answering.live_inventory.as_deref() {
            let len = live.0.item_count();
            ui.label(format!("Your inventory: {len}/{cap}"));
            if len >= cap {
                // A real action, not a panel flag (#1220 f288). This
                // modal blocks background input, so the old button
                // raised the Inventory UNDER it - unclickable, while the
                // countdown declined the gift out from under the user.
                // The offer is held instead: the dialog closes, the
                // Inventory works, and the offer comes straight back the
                // moment a slot frees.
                if ui
                    .button("Make room for it")
                    .on_hover_text(
                        "Sets this offer aside and opens your Inventory. It comes \
                         back as soon as you free a slot - the sender's countdown \
                         keeps running.",
                    )
                    .clicked()
                {
                    action = Some(OfferAction::Hold);
                }
            }
        }
        ui.add_space(6.0);
        let blocked = accept_blocked_reason(
            &dialog.item,
            answering.live_inventory.as_deref().map(|live| &live.0),
            answering
                .session
                .as_deref()
                .map(|s| s.session.token_set().scope.clone())
                .as_deref(),
        );
        if let Some(asked) = answer_row(ui, blocked) {
            action = Some(asked);
        }
        ui.add_space(4.0);
        // The lifecycle sweep auto-declines the dialog after the TTL
        // (`config::network::OFFER_DIALOG_TIMEOUT_SECS`) - surface that
        // instead of letting the offer vanish invisibly mid-decision.
        // Read off the same wall clock the sweep uses (#1216). On the
        // virtual clock this number was not seconds: "declines in 40s" did
        // not advance at all while the window was backgrounded, so it
        // disagreed with both the sweep and the sender.
        let remaining = (crate::config::network::OFFER_DIALOG_TIMEOUT_SECS
            - crate::state::real_secs_since(dialog.arrived_at_epoch))
        .max(0.0)
        .ceil() as u64;
        ui.small(format!(
            "Declines automatically in {remaining}s - Esc to decline now."
        ));
    });
    // Esc (or a click on the dimmed backdrop) = Decline: the safe,
    // non-destructive dismissal - the sender gets an honest response
    // instead of a dialog that lingers until the TTL sweep.
    if action.is_none() && modal.should_close() {
        action = Some(OfferAction::Decline);
    }

    let Some(action) = action else {
        return;
    };

    let answer = match action {
        OfferAction::Accept => OfferAnswer::Accept { publish: true },
        OfferAction::Decline => OfferAnswer::Decline,
        OfferAction::MuteAndDecline => OfferAnswer::MuteAndDecline,
        // Held, not answered (#1220 f288): the sender hears nothing yet, and
        // `resolve_held_offer` owns both of the hold's ends. Returns before
        // the busy-decline note in `answer`, which belongs to a dialog that
        // is closing for good.
        OfferAction::Hold => {
            panels.inventory = true;
            let now = answering.time.elapsed_secs_f64();
            answering.toasts.info(
                format!(
                    "\"{}\" is held - free a slot and it will come back.",
                    dialog.item_name
                ),
                now,
            );
            answering
                .commands
                .insert_resource(crate::state::HeldOffer((*dialog).clone()));
            answering.commands.remove_resource::<IncomingOfferDialog>();
            return;
        }
    };
    answering.answer(&dialog, answer);
}

impl OfferAnswering<'_, '_> {
    /// Answer `dialog`: tell its sender, and on an accept put the gift in
    /// the live inventory - saving it at once when `publish` says to - and
    /// say where it landed. `None` for anything but an accepted gift that
    /// landed.
    pub(crate) fn answer(
        &mut self,
        dialog: &IncomingOfferDialog,
        answer: OfferAnswer,
    ) -> Option<AcceptedGift> {
        let now = self.time.elapsed_secs_f64();
        // The dialog is closing (#843): report offers the busy-gate silently
        // turned away while the user decided, then reset for the next one.
        if self.busy_declines.0 > 0 {
            let turned_away = self.busy_declines.0;
            self.toasts.info(
                format!(
                    "{} more offer{} arrived while you decided and {} auto-declined.",
                    turned_away,
                    if turned_away == 1 { "" } else { "s" },
                    if turned_away == 1 { "was" } else { "were" },
                ),
                now,
            );
            self.busy_declines.0 = 0;
        }
        let accepted = matches!(answer, OfferAnswer::Accept { .. });
        // Count the local user's offer disposition (E-4) - accept vs any decline.
        if accepted {
            crate::diagnostics::samplers::offer_accepted(&mut self.metrics);
        } else {
            crate::diagnostics::samplers::offer_declined(&mut self.metrics);
        }

        // Flip the mute flag on the sender's `RemotePeer` before we send the
        // response so any subsequent offer this frame (unlikely but possible
        // if the attacker double-sent) is already auto-declined as muted.
        if matches!(answer, OfferAnswer::MuteAndDecline) {
            // Hoisted OUT of the peer loop (#1219 f120). This used to write the
            // durable list from inside `for peer in peers.iter_mut()`, so a
            // stranger who spammed a gift and disconnected - the hit-and-run case
            // the durable list exists for - matched nothing and was never
            // recorded, and their next visit reached the user exactly as before.
            // The dialog's sender DID is relay-authenticated; the comment here
            // always said it was safe to key on unconditionally, and now it is.
            let live = self
                .peers
                .iter_mut()
                .find(|peer| peer.peer_id == dialog.sender_peer_id);
            crate::network::presence::set_peer_mute(
                live.map(Mut::into_inner),
                Some(dialog.sender_did.as_str()),
                true,
                &mut self.muted_dids,
                &mut self.session_log,
                Some(dialog.sender_peer_id),
                now,
            );
        }

        let mut landed = None;
        // A world is answered when its parts land, not at the click (#1844).
        let mut answered_later = false;
        if let OfferAnswer::Accept { publish } = answer {
            let item = match &dialog.item {
                GiftItem::World(head) => {
                    self.fetch_given_world(dialog, head, publish, now);
                    answered_later = true;
                    None
                }
                GiftItem::Generator { generator, wear } => {
                    Some(crate::ui::inventory::GiftLanding::Generator {
                        generator: generator.clone(),
                        wear: wear.clone(),
                    })
                }
                GiftItem::Avatar(avatar) => {
                    Some(crate::ui::inventory::GiftLanding::Avatar(avatar.clone()))
                }
            };
            if let Some(item) = item {
                landed = self.land(dialog, item, publish, now);
            }
        } else {
            self.session_log.info(
                now,
                EventPayload::ItemOfferUserResponded {
                    offer_id: dialog.offer_id,
                    accepted: false,
                },
            );
        }

        // Fire the response back to the sender. Broadcast-with-address: the
        // `target_did` field is the *sender's* DID so only they pick it up.
        if !answered_later {
            self.writer.write(Broadcast {
                payload: OverlandsMessage::item_offer_response(
                    dialog.offer_id,
                    dialog.sender_did.clone(),
                    accepted,
                    // A person answered (#1220 f127) - including "Mute &
                    // Decline", which reports as a plain decline for privacy.
                    crate::protocol::DeclineReason::Declined,
                ),
                channel: ChannelKind::Reliable,
            });
        }

        self.commands.remove_resource::<IncomingOfferDialog>();
        landed
    }

    /// Land an accepted thing or avatar in the live inventory by the one
    /// rule every gift lands by ([`crate::ui::inventory::accept_gift`]),
    /// saving it at once when `publish` says to, and say where it landed.
    fn land(
        &mut self,
        dialog: &IncomingOfferDialog,
        item: crate::ui::inventory::GiftLanding,
        publish: bool,
        now: f64,
    ) -> Option<AcceptedGift> {
        let Some(live) = self.live_inventory.as_mut() else {
            // Live inventory resource absent - should not happen in
            // `AppState::InGame`, but answer rather than drop the response
            // and leave the sender hanging. The user's response was an
            // accept, so it records as such but at Warn severity because the
            // item could not actually be stored.
            warn!(
                "Could not store accepted offer \"{}\" from {}: inventory not loaded",
                dialog.item_name,
                dialog.sender_label.addressed()
            );
            self.session_log.warn(
                now,
                EventPayload::ItemOfferUserResponded {
                    offer_id: dialog.offer_id,
                    accepted: true,
                },
            );
            return None;
        };
        // The gift lands in `live`; what gets PUBLISHED is `stored` plus the
        // gift (#1200) - the owner's other unsaved edits are theirs to save
        // or revert, not this dialog's to commit.
        let stored = self
            .stored_inventory
            .as_deref()
            .map(|s| s.0.clone())
            .unwrap_or_default();
        // Bind the landed key (#1220 f119). `accept_gift` renames on a
        // collision - "lantern" becomes "lantern_2" - and discarding the key
        // meant the one moment a gift becomes yours was the least confirmed
        // event in the lifecycle, under a name the recipient was never shown.
        let (key, payload) =
            crate::ui::inventory::accept_gift(&mut live.0, &stored, &dialog.item_name, item);
        self.session_log.info(
            now,
            EventPayload::ItemOfferUserResponded {
                offer_id: dialog.offer_id,
                accepted: true,
            },
        );
        let saving = crate::ui::inventory::after_gift_landed(
            crate::ui::inventory::Landed {
                key: &key,
                offered: &dialog.item_name,
                payload,
                stored,
                publish,
            },
            &mut crate::ui::inventory::LandingSinks {
                commands: &mut self.commands,
                session: self.session.as_deref(),
                refresh: self.refresh_ctx.as_deref(),
                recovery: self.inventory_recovery.is_some(),
                feedback: self.inventory_feedback.reborrow(),
                toasts: &mut self.toasts,
            },
            now,
        );
        Some(AcceptedGift { key, saving })
    }

    /// Start fetching an accepted world from the giver's saved copy (#1844):
    /// from the data server of the relay-authenticated sender the offer
    /// arrived under, never any DID the payload names. It lands, and the
    /// sender hears the answer, when the parts arrive.
    fn fetch_given_world(
        &mut self,
        dialog: &IncomingOfferDialog,
        head: &crate::pds::inventory::WorldHead,
        publish: bool,
        now: f64,
    ) {
        let Some(recipient) = self.session.as_deref().map(|s| s.did.clone()) else {
            warn!("A world was accepted with nobody signed in; nothing was fetched");
            return;
        };
        let fetch = self.gift_fetch.as_deref().cloned().unwrap_or_default();
        crate::ui::inventory::gift_world::spawn_gift_world_task(
            &mut self.commands,
            &fetch,
            crate::ui::inventory::gift_world::GiftWorldOffer {
                offer_id: dialog.offer_id,
                giver_did: &dialog.sender_did,
                giver_label: dialog.sender_label.addressed(),
                recipient_did: &recipient,
                name: &dialog.item_name,
                head,
                publish,
                arrived_at_epoch: dialog.arrived_at_epoch,
            },
            now,
        );
        self.toasts.info(
            format!(
                "Fetching \"{}\" from {}'s saved copy\u{2026}",
                dialog.item_name,
                dialog.sender_label.addressed()
            ),
            now,
        );
    }
}

/// How large an offered item is (#843, #1844): a thing's or an avatar's
/// serialised size, a world's head's word for its saved JSON.
fn offer_bytes(item: &GiftItem) -> Option<usize> {
    match item {
        GiftItem::Generator { generator, .. } => {
            crate::pds::record_size::serialized_record_bytes(generator)
        }
        GiftItem::Avatar(avatar) => crate::pds::record_size::serialized_record_bytes(avatar),
        GiftItem::World(head) => usize::try_from(head.bytes).ok(),
    }
}

/// What kind of thing is offered, as a person reads it (#1267 f214, #1844):
/// a thing's kind, or an avatar's or a world's.
fn offer_kind_label(item: &GiftItem) -> String {
    match item {
        // #1267 f214: a visitor deciding whether to accept a gift was told
        // what kind of thing it is as "BlobGroup".
        GiftItem::Generator { generator, .. } => {
            crate::pds::GeneratorKind::display_name(generator.kind_tag()).to_owned()
        }
        GiftItem::Avatar(avatar) => format!("Avatar \u{b7} {}", avatar.kind().label()),
        GiftItem::World(head) => format!("World \u{b7} {}", head.summary.kind_label()),
    }
}

/// What accepting a whole item does, said before the click (#1844): an
/// avatar goes into the Inventory and the one worn stays on; a world is
/// fetched from the giver's saved copy and changes nothing in the
/// recipient's own world. `None` for a thing.
fn offer_note(item: &GiftItem, giver: &PeerLabel) -> Option<String> {
    match item {
        GiftItem::Generator { .. } => None,
        GiftItem::Avatar(_) => Some(String::from(
            "It goes into your Inventory - the avatar you have on stays on.",
        )),
        GiftItem::World(_) => {
            let whose = if giver.is_named() {
                format!("{}'s", giver.addressed())
            } else {
                String::from("the sender's")
            };
            Some(format!(
                "Its contents are fetched from {whose} saved copy when you accept, and it goes \
                 into your Inventory - nothing in your own world changes."
            ))
        }
    }
}

/// Why Accept cannot take `item` right now, or `None` when it can
/// (#1220 f302, #1844): the stash has not loaded, or is full, or - for a
/// world, whose landing saves records of a collection of their own - the
/// sign-in's grant `scope` cannot write them
/// ([`crate::pds::inventory::world::grant_covers_parts`]). Decline and Mute &
/// Decline work whatever this says.
pub(crate) fn accept_blocked_reason(
    item: &GiftItem,
    live: Option<&InventoryRecord>,
    scope: Option<&str>,
) -> Option<&'static str> {
    let Some(live) = live else {
        return Some("Your inventory hasn't loaded - nothing can be accepted into it yet.");
    };
    if live.is_full() {
        return Some("Your inventory is full - free a slot with \"Make room for it\".");
    }
    accept_grant_refusal(item, scope)
}

/// Why a sign-in whose grant is `scope` cannot accept `item`, or `None`
/// when it can (#1844): a world's landing saves records of a collection of
/// their own, which a grant from before it cannot write. The part of
/// [`accept_blocked_reason`] the agent client asks on its own (#1847),
/// where the stash's other answers are its own words.
pub(crate) fn accept_grant_refusal(item: &GiftItem, scope: Option<&str>) -> Option<&'static str> {
    (matches!(item, GiftItem::World(_))
        && scope.is_some_and(|scope| !crate::pds::inventory::world::grant_covers_parts(scope)))
    .then_some(crate::pds::inventory::world::SIGN_IN_AGAIN)
}

/// The modal's answers (#1220 f302, #1844): Accept - disabled when
/// `accept_blocked` says why, and the reason PAINTED above it, since egui
/// shows no hover text on a disabled control (#1289) and the user is on a
/// countdown - then Decline and Mute & Decline, which always work. A widget
/// function so a test can drive it.
fn answer_row(ui: &mut egui::Ui, accept_blocked: Option<&str>) -> Option<OfferAction> {
    let mut action = None;
    if let Some(reason) = accept_blocked {
        ui.add(
            egui::Label::new(
                egui::RichText::new(reason).color(crate::ui::theme::current(ui.ctx()).status.error),
            )
            .wrap(),
        );
    }
    ui.horizontal(|ui| {
        let accept = ui
            .add_enabled(
                accept_blocked.is_none(),
                egui::Button::new(
                    egui::RichText::new("Accept")
                        .color(crate::ui::theme::current(ui.ctx()).status.ok),
                ),
            )
            .on_disabled_hover_text(accept_blocked.unwrap_or_default());
        if accept.clicked() {
            action = Some(OfferAction::Accept);
        }
        if ui.button("Decline").clicked() {
            action = Some(OfferAction::Decline);
        }
        if ui
            .add(egui::Button::new(
                egui::RichText::new("Mute & Decline")
                    .color(crate::ui::theme::current(ui.ctx()).status.error),
            ))
            .clicked()
        {
            action = Some(OfferAction::MuteAndDecline);
        }
    });
    action
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OfferAction {
    Accept,
    Decline,
    MuteAndDecline,
    /// Set the offer aside and open the Inventory (#1220 f288). Not an
    /// answer: no response goes to the sender, and the offer returns when a
    /// slot frees or is declined when its clock runs out - see
    /// `network::lifecycle::resolve_held_offer`.
    Hold,
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use bevy_symbios_multiuser::prelude::PeerId;

    const THING: GiftKind = GiftKind::Generator;

    /// A peer whose build reads `compat` at `NOW`.
    const NOW: f64 = 10.0;
    fn on_build(compat: Option<u16>, connected_at: f64) -> RemotePeer {
        RemotePeer {
            build: compat.map(|protocol| crate::state::PeerBuild {
                protocol,
                build: String::from("0.8.0+abcdef1"),
            }),
            connected_at,
            ..peer(Some("did:plc:them"), false)
        }
    }

    /// #1844. A whole avatar or world goes only to a build that can take
    /// one: not to a peer still announcing, nor to one that never did, nor
    /// to one on protocol 5 - each refused with its own sentence - and to
    /// protocol 6 or later. A thing goes to all four, as it always has.
    #[test]
    fn a_whole_gift_waits_for_a_build_that_can_take_it() {
        use crate::state::PeerCompatibility;
        let pending = on_build(None, NOW);
        let unannounced = on_build(None, 0.0);
        let five = on_build(Some(5), 0.0);
        let six = on_build(Some(6), 0.0);
        let seven = on_build(Some(7), 0.0);
        assert_eq!(pending.compatibility(NOW), PeerCompatibility::Pending);
        assert_eq!(
            unannounced.compatibility(NOW),
            PeerCompatibility::Unannounced
        );
        assert_eq!(five.compatibility(NOW), PeerCompatibility::Mismatched(5));
        // A build a version behind, on 6 since #1861's 7, still takes one.
        assert_eq!(six.compatibility(NOW), PeerCompatibility::Mismatched(6));
        assert_eq!(seven.compatibility(NOW), PeerCompatibility::Compatible);

        for whole in [GiftKind::Avatar, GiftKind::World] {
            let still = gift_block_reason(&pending, true, whole, NOW).expect("waits");
            assert!(still.contains("try again in a moment"), "{still}");
            for older in [&unannounced, &five] {
                let why = gift_block_reason(older, true, whole, NOW).expect("refused");
                assert!(why.contains("older version"), "{why}");
                assert!(why.contains("whole avatar or world"), "{why}");
            }
            assert_eq!(gift_block_reason(&six, true, whole, NOW), None);
            assert_eq!(gift_block_reason(&seven, true, whole, NOW), None, "newer");
        }
        for peer in [&pending, &unannounced, &five, &six] {
            assert_eq!(gift_block_reason(peer, true, THING, NOW), None);
        }
        // The older gates still come first.
        let muted = RemotePeer {
            muted: true,
            ..on_build(Some(5), 0.0)
        };
        assert!(
            gift_block_reason(&muted, true, GiftKind::World, NOW)
                .is_some_and(|why| why.contains("muted"))
        );
    }

    /// The painted text one frame of [`answer_row`] drew, and what it was
    /// asked for, with the pointer events given.
    fn answer_frame(
        ctx: &egui::Context,
        frame: &mut u32,
        blocked: Option<&str>,
        events: Vec<egui::Event>,
    ) -> (Option<OfferAction>, Vec<String>, Vec<(String, egui::Pos2)>) {
        *frame += 1;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            time: Some(f64::from(*frame) / 60.0),
            events,
            ..Default::default()
        };
        let mut action = None;
        let output = ctx.run_ui(input, |ui| {
            action = answer_row(ui, blocked);
        });
        fn texts_of(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts_of(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            texts_of(&clipped.shape, &mut texts);
        }
        let nodes = output
            .platform_output
            .accesskit_update
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| {
                        let label = node.label()?.to_string();
                        let b = node.bounds()?;
                        Some((
                            label,
                            egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        (action, texts, nodes)
    }

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Click the button labelled `label` in [`answer_row`] and say what it
    /// asked for.
    fn click(blocked: Option<&str>, label: &str) -> (Option<OfferAction>, Vec<String>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut frame = 0;
        let (_, painted, nodes) = answer_frame(&ctx, &mut frame, blocked, vec![]);
        let at = nodes
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, at)| *at)
            .unwrap_or_else(|| panic!("the row draws {label}: {nodes:?}"));
        answer_frame(
            &ctx,
            &mut frame,
            blocked,
            vec![egui::Event::PointerMoved(at)],
        );
        answer_frame(&ctx, &mut frame, blocked, vec![press(at, true)]);
        let (action, _, _) = answer_frame(&ctx, &mut frame, blocked, vec![press(at, false)]);
        (action, painted)
    }

    /// #1844, #1289. A sign-in whose grant cannot write a world's parts
    /// cannot accept a world: Accept does nothing, and the reason is PAINTED,
    /// since egui shows no hover on a disabled control, in the words a save
    /// would refuse it with. Decline and Mute & Decline still work, and a
    /// thing is not refused by the same grant.
    #[test]
    fn accept_of_a_world_is_disabled_and_says_why_on_a_grant_without_parts() {
        let head = crate::pds::inventory::WorldItem::snapshot(
            &crate::pds::inventory::world::tests::seeded_world(61),
        )
        .expect("copies")
        .head()
        .clone();
        let world = GiftItem::World(head);
        let a_thing = GiftItem::Generator {
            generator: Box::new(crate::pds::Generator::default_cuboid()),
            wear: None,
        };
        let stash = InventoryRecord::default();
        let old_grant = format!("atproto repo:{}", crate::pds::INVENTORY_ITEM_COLLECTION);
        let new_grant = format!("{old_grant} repo:{}", crate::pds::INVENTORY_PART_COLLECTION);
        let reason =
            accept_blocked_reason(&world, Some(&stash), Some(&old_grant)).expect("refused");
        assert_eq!(reason, crate::pds::inventory::world::SIGN_IN_AGAIN);
        assert_eq!(
            accept_blocked_reason(&world, Some(&stash), Some(&new_grant)),
            None
        );
        assert_eq!(
            accept_blocked_reason(&a_thing, Some(&stash), Some(&old_grant)),
            None
        );
        assert!(
            accept_blocked_reason(&a_thing, None, None).is_some(),
            "not loaded"
        );

        let (accepted, painted) = click(Some(reason), "Accept");
        assert_eq!(accepted, None, "a disabled Accept does nothing");
        assert!(
            painted.iter().any(|text| text == reason),
            "the reason is painted, not hidden in a hover: {painted:?}"
        );
        assert_eq!(click(Some(reason), "Decline").0, Some(OfferAction::Decline));
        assert_eq!(
            click(Some(reason), "Mute & Decline").0,
            Some(OfferAction::MuteAndDecline)
        );
        let (accepted, painted) = click(None, "Accept");
        assert_eq!(accepted, Some(OfferAction::Accept));
        assert!(!painted.iter().any(|text| text == reason));
    }

    /// #1844. The modal names what is offered and how large - an avatar's
    /// kind and size, a world's kind and its head's size - and what Accept
    /// does to the recipient's own avatar and world: nothing.
    #[test]
    fn the_offer_says_what_a_whole_item_is_and_what_accepting_it_does() {
        use crate::pds::inventory::avatar_item_tests::rigged_record;
        let avatar =
            crate::pds::inventory::AvatarItem::from_record(&rigged_record()).expect("copies");
        let item = GiftItem::Avatar(Box::new(avatar.clone()));
        assert_eq!(offer_kind_label(&item), "Avatar \u{b7} person");
        assert_eq!(
            offer_bytes(&item),
            crate::pds::record_size::serialized_record_bytes(&avatar)
        );
        let giver = PeerLabel::new(Some("giver.test"), Some("did:plc:giver"));
        assert!(offer_note(&item, &giver).is_some_and(|note| note.contains("stays on")));

        let head = crate::pds::inventory::WorldItem::snapshot(
            &crate::pds::inventory::world::tests::seeded_world(62),
        )
        .expect("copies")
        .head()
        .clone();
        let world = GiftItem::World(head.clone());
        assert_eq!(offer_kind_label(&world), "World \u{b7} a procedural world");
        assert_eq!(offer_bytes(&world), Some(head.bytes as usize));
        let note = offer_note(&world, &giver).expect("a note");
        assert!(note.contains("@giver.test's saved copy"), "{note}");
        assert!(note.contains("nothing in your own world changes"), "{note}");
        let a_thing = GiftItem::Generator {
            generator: Box::new(crate::pds::Generator::default_cuboid()),
            wear: None,
        };
        assert_eq!(offer_note(&a_thing, &giver), None);
    }

    fn peer_id(last: u8) -> PeerId {
        serde_json::from_str::<PeerId>(&format!("\"00000000-0000-0000-0000-0000000000{last:02}\""))
            .expect("a well-formed uuid")
    }

    fn peer(did: Option<&str>, muted: bool) -> RemotePeer {
        RemotePeer {
            peer_id: peer_id(1),
            did: did.map(str::to_owned),
            handle: None,
            muted,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    fn named(id: u8, handle: Option<&str>, did: Option<&str>) -> RemotePeer {
        RemotePeer {
            peer_id: peer_id(id),
            did: did.map(str::to_owned),
            handle: handle.map(str::to_owned),
            muted: false,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    /// #1226 f325 (the half that survived its refuter: the ROW already
    /// renders distinct DID heads, but the SORT still put every stranger
    /// under one key). The sequence: two people join, neither handle has
    /// resolved, and their rows are tied - so the order between them is
    /// whatever the last component insert left in archetype order, and it
    /// can change under a pointer that is on its way to a Mute checkbox.
    #[test]
    fn two_unresolved_peers_no_longer_sort_to_the_same_key() {
        let a = named(1, None, Some("did:plc:aaaaaaaaaaaaaaaa"));
        let b = named(2, None, Some("did:plc:bbbbbbbbbbbbbbbb"));
        assert_ne!(roster_sort_key(&a, None), roster_sort_key(&b, None));
        assert!(
            roster_sort_key(&a, None) < roster_sort_key(&b, None),
            "and they order by the DID head the row actually prints"
        );
    }

    /// Even two peers who never identified at all - no handle, no DID, the
    /// same rendered "A traveler" - hold a stable order, because the peer id
    /// is the last tier and it is unique per connection.
    #[test]
    fn two_anonymous_peers_still_hold_a_stable_order() {
        let a = named(3, None, None);
        let b = named(4, None, None);
        assert!(roster_sort_key(&a, None) < roster_sort_key(&b, None));
    }

    /// The tiers are separated rather than concatenated: every DID head
    /// begins `did:plc:`, so folded into one string every stranger would
    /// bunch among the handles beginning with `d` instead of below them.
    #[test]
    fn a_named_person_sorts_above_every_stranger() {
        let named_late = named(5, Some("zoe.bsky.social"), Some("did:plc:zzzzzzzz"));
        let stranger = named(6, None, Some("did:plc:aaaaaaaa"));
        let anon = named(7, None, None);
        assert!(roster_sort_key(&named_late, None) < roster_sort_key(&stranger, None));
        assert!(roster_sort_key(&stranger, None) < roster_sort_key(&anon, None));
    }

    /// Mutuals stay first, ahead of the ladder - the deliberate order #844
    /// established and the gateway picker shares.
    #[test]
    fn a_mutual_outranks_an_alphabetically_earlier_stranger() {
        let mutual = named(8, Some("zoe.bsky.social"), Some("did:plc:zzzzzzzz"));
        let other = named(9, Some("aaron.bsky.social"), Some("did:plc:aaaaaaaa"));
        assert!(
            roster_sort_key(&mutual, Some(&SocialResonance::Mutual))
                < roster_sort_key(&other, None)
        );
    }

    /// #1220 f330. The sequence: someone joins, you drag a lamp onto their
    /// row before their DID has resolved, release - and absolutely nothing
    /// happens. No highlight during the drag, no toast on release, because
    /// an ineligible row was never recorded as a target and the drop fell
    /// through to the silent cancel written for releases over the Inventory
    /// window. That transient is the most likely state for a new user's
    /// FIRST gift.
    #[test]
    fn every_reason_a_row_cannot_take_a_gift_has_a_sentence() {
        assert_eq!(
            gift_block_reason(&peer(Some("did:plc:them"), false), true, THING, 0.0),
            None
        );

        let unidentified =
            gift_block_reason(&peer(None, false), true, THING, 0.0).expect("blocked");
        assert!(unidentified.contains("identifying"), "{unidentified}");
        let muted = gift_block_reason(&peer(Some("did:plc:them"), true), true, THING, 0.0)
            .expect("blocked");
        assert!(muted.contains("muted"), "{muted}");
        let offline = gift_block_reason(&peer(Some("did:plc:them"), false), false, THING, 0.0)
            .expect("blocked");
        assert!(offline.contains("connected"), "{offline}");
    }

    /// A muted peer is muted whether or not they identified, and telling
    /// somebody "still identifying" about a person they deliberately blocked
    /// is the wrong sentence.
    #[test]
    fn a_mute_outranks_a_missing_identity_in_the_gift_reason() {
        let reason = gift_block_reason(&peer(None, true), true, THING, 0.0).expect("blocked");
        assert!(reason.contains("muted"), "{reason}");
    }

    /// #1220 f302. Five states used to make the Visit button VANISH, and an
    /// absent control cannot carry a tooltip. Three are visible elsewhere;
    /// "you're already in their world" was the one with no cue anywhere on
    /// screen.
    #[test]
    fn every_reason_visit_is_unavailable_has_a_sentence() {
        let them = peer(Some("did:plc:them"), false);
        assert_eq!(visit_block_reason(&them, false, false, false), None);

        assert!(
            visit_block_reason(&them, true, false, false)
                .expect("blocked")
                .contains("already in their world")
        );
        assert!(
            visit_block_reason(&them, false, true, false)
                .expect("blocked")
                .contains("travelling")
        );
        assert!(
            visit_block_reason(&them, false, false, true)
                .expect("blocked")
                .contains("unsaved")
        );
        assert!(
            visit_block_reason(&peer(Some("did:plc:them"), true), false, false, false)
                .expect("blocked")
                .contains("muted")
        );
        assert!(
            visit_block_reason(&peer(None, false), false, false, false)
                .expect("blocked")
                .contains("identifying")
        );
    }
}
