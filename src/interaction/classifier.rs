//! Per-frame contact producer for the interaction framework.
//!
//! Joins the otherwise-disjoint local-player and remote-peer queries
//! into one iteration, derives a per-avatar velocity (direct from
//! `LinearVelocity` for locals, 1-frame finite-difference for peers
//! whose components do not currently carry the velocity), probes each
//! surface registry, and emits [`ContactSample`]s into the
//! [`AvatarContacts`] resource — clearing it first so consumers only
//! see this frame's data.
//!
//! Two internal state caches survive across frames:
//!
//! - [`PeerVelocityCache`] — last frame's `(position, time)` per peer
//!   entity, used for the finite-difference velocity fallback. Entries
//!   for despawned peers are pruned each frame.
//! - [`ContactPersistence`] — last frame's [`SurfaceContact`] per
//!   avatar, used to emit `Enter` / `Dwell` / `Exit` phases
//!   automatically. A despawned avatar's last surface is silently
//!   dropped — we don't synthesise an `Exit` for a vanished entity
//!   because the consumer would have no place to spawn an effect.
//!
//! The pure logic (probe, phase computation, intensity) is split out so
//! tests don't have to build a Bevy app.

use std::collections::HashMap;

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use crate::config::terrain::water::wake as wcfg;
use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::water::WaterSurfaces;

use super::contact::{AvatarContacts, ContactPhase, ContactSample, SurfaceContact};
use super::locomotion::{locomotion_footprint, locomotion_total_height};

/// Last-frame position/time cache for remote peers (whose entities do
/// not carry `LinearVelocity`). Pruned each frame to avoid leaking
/// memory for despawned peers.
#[derive(Resource, Default, Debug)]
pub struct PeerVelocityCache {
    pub(crate) entries: HashMap<Entity, (Vec3, f32)>,
}

/// Last-frame surface contact per avatar entity. Used to compute the
/// `ContactPhase` of the current frame's sample by comparing against
/// the just-classified surface. Entries are removed when an avatar
/// transitions to "no contact" so the next contact starts with
/// `ContactPhase::Enter`.
#[derive(Resource, Default, Debug)]
pub struct ContactPersistence {
    pub(crate) last_surface: HashMap<Entity, SurfaceContact>,
}

/// One transition the classifier might emit for an avatar this frame.
/// In the case of a surface-kind change (water plane A → water plane
/// B, or eventually water → terrain) two transitions are emitted: an
/// `Exit` for the old surface and an `Enter` for the new one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Transition {
    pub surface: SurfaceContact,
    pub phase: ContactPhase,
}

/// Probe every surface registry against an avatar and return the
/// strongest contact, or `None` if the avatar isn't touching any
/// tracked surface. Phase 0 only probes water; Phase 3 extends this to
/// add a terrain raycast.
///
/// Contact is tested at the avatar's **body bottom**
/// (`world_pos.y − total_height/2`), not the chassis origin. Probing
/// the origin made emission razor-sensitive to height — a vehicle
/// whose origin floats above the waterline (hull in the water, origin
/// not) produced nothing, since `WaterSurfaces::query` culls points
/// above the surface. Probing the body bottom with `query_signed`
/// (which keeps above-surface hits) mirrors the established
/// `humanoid_water_state` feet-vs-surface test and makes emission
/// depend on the avatar's vertical extent.
///
/// The waterline test is a **Schmitt trigger**: `was_in_contact`
/// selects the threshold. Entering uses the tight
/// [`wcfg::CONTACT_SLACK`]; once in contact the avatar stays in
/// contact until its body bottom rises past the much wider
/// [`wcfg::CONTACT_EXIT_SLACK`]. The hysteresis band absorbs the
/// settling bob of a decelerating hull — without it the body bottom
/// chatters across a single threshold every frame, flipping
/// Exit→Enter and spawning a burst of splash rings as the boat halts.
///
/// `depth` in the returned [`SurfaceContact::Water`] is the body
/// bottom's submersion (clamped ≥ 0), so the downstream
/// `intensity_for` (depth ÷ total_height) still reads 0 at the
/// waterline and saturates when fully submerged.
pub(crate) fn probe_surface(
    world_pos: Vec3,
    total_height: f32,
    was_in_contact: bool,
    water: &WaterSurfaces,
) -> Option<SurfaceContact> {
    let body_bottom = Vec3::new(world_pos.x, world_pos.y - 0.5 * total_height, world_pos.z);
    let q = water.query_signed(body_bottom)?;
    // `q.depth` is positive when the body bottom is submerged. The
    // reject threshold widens once already in contact (hysteresis):
    // tight to enter, generous to leave.
    let slack = if was_in_contact {
        wcfg::CONTACT_EXIT_SLACK
    } else {
        wcfg::CONTACT_SLACK
    };
    if q.depth < -slack {
        return None;
    }
    Some(SurfaceContact::Water {
        plane_idx: q.surface_idx,
        depth: q.depth.max(0.0),
        flow_dir: Vec2::new(q.flow_dir.x, q.flow_dir.z),
    })
}

/// Pure phase-transition logic. Independent of the rest of the system
/// so it can be exhaustively unit-tested without spinning up a Bevy
/// `World`.
///
/// Returns a small vector (≤ 2 elements) of transitions to emit and
/// the new persistence value to store. A `None` second tuple element
/// means "drop the entry from persistence" (avatar left every
/// surface).
pub(crate) fn compute_transitions(
    last: Option<SurfaceContact>,
    curr: Option<SurfaceContact>,
) -> (Vec<Transition>, Option<SurfaceContact>) {
    match (last, curr) {
        (None, None) => (Vec::new(), None),
        (None, Some(c)) => (
            vec![Transition {
                surface: c,
                phase: ContactPhase::Enter,
            }],
            Some(c),
        ),
        (Some(p), None) => (
            vec![Transition {
                surface: p,
                phase: ContactPhase::Exit,
            }],
            None,
        ),
        (Some(p), Some(c)) => {
            if same_specific_surface(&p, &c) {
                (
                    vec![Transition {
                        surface: c,
                        phase: ContactPhase::Dwell,
                    }],
                    Some(c),
                )
            } else {
                (
                    vec![
                        Transition {
                            surface: p,
                            phase: ContactPhase::Exit,
                        },
                        Transition {
                            surface: c,
                            phase: ContactPhase::Enter,
                        },
                    ],
                    Some(c),
                )
            }
        }
    }
}

/// "Same surface" for phase-tracking purposes: same kind AND same
/// surface-specific key (water plane index, eventually terrain region
/// id). A change in `depth` does not count as a surface change —
/// that's what `Dwell` is for.
fn same_specific_surface(a: &SurfaceContact, b: &SurfaceContact) -> bool {
    if a.kind() != b.kind() {
        return false;
    }
    match (a, b) {
        (
            SurfaceContact::Water { plane_idx: ai, .. },
            SurfaceContact::Water { plane_idx: bi, .. },
        ) => ai == bi,
    }
}

/// 0..1 engagement scalar driven by the surface-specific payload. For
/// water this is depth normalised by avatar height — fully submerged
/// reads as 1.0. The caller's surface enum determines the formula.
pub(crate) fn intensity_for(contact: &SurfaceContact, locomotion: &LocomotionConfig) -> f32 {
    match contact {
        SurfaceContact::Water { depth, .. } => {
            let denom = locomotion_total_height(locomotion).max(0.01);
            (depth / denom).clamp(0.0, 1.0)
        }
    }
}

/// Per-frame system: rebuild [`AvatarContacts`] from the current state
/// of every avatar entity.
#[allow(clippy::too_many_arguments)]
pub fn classify_contacts(
    time: Res<Time>,
    water: Option<Res<WaterSurfaces>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    locals: Query<(Entity, &Transform, &LinearVelocity), With<LocalPlayer>>,
    peers: Query<(Entity, &Transform, &RemotePeer), Without<LocalPlayer>>,
    mut peer_vel_cache: ResMut<PeerVelocityCache>,
    mut persistence: ResMut<ContactPersistence>,
    mut contacts: ResMut<AvatarContacts>,
) {
    contacts.samples.clear();

    let Some(water) = water.as_deref() else {
        // No water registry yet (still loading) — keep persistence
        // intact so a partial frame doesn't fabricate an Exit, and
        // skip emission entirely.
        return;
    };

    let elapsed = time.elapsed_secs();
    let unknown = LocomotionConfig::Unknown;

    // ----- Local players (direct LinearVelocity) -----
    let local_cfg: &LocomotionConfig = live_avatar
        .as_ref()
        .map(|r| &r.0.locomotion)
        .unwrap_or(&unknown);
    let local_footprint = locomotion_footprint(local_cfg);
    for (entity, transform, lin_vel) in locals.iter() {
        emit_for_avatar(
            entity,
            transform.translation,
            lin_vel.0,
            local_footprint,
            local_cfg,
            water,
            &mut persistence,
            &mut contacts.samples,
        );
    }

    // ----- Remote peers (1-frame finite-difference fallback) -----
    let mut alive_peers: Vec<Entity> = Vec::with_capacity(peers.iter().len());
    for (entity, transform, peer) in peers.iter() {
        alive_peers.push(entity);

        let curr_pos = transform.translation;
        let vel = if let Some((prev_pos, prev_time)) = peer_vel_cache.entries.get(&entity).copied()
        {
            let dt = (elapsed - prev_time).max(1e-4);
            (curr_pos - prev_pos) / dt
        } else {
            Vec3::ZERO
        };
        peer_vel_cache.entries.insert(entity, (curr_pos, elapsed));

        let cfg = peer
            .avatar
            .as_ref()
            .map(|a| &a.locomotion)
            .unwrap_or(&unknown);
        let footprint = locomotion_footprint(cfg);
        emit_for_avatar(
            entity,
            curr_pos,
            vel,
            footprint,
            cfg,
            water,
            &mut persistence,
            &mut contacts.samples,
        );
    }

    // Prune velocity-cache entries for despawned peers so the cache
    // can't grow unbounded across long sessions.
    peer_vel_cache
        .entries
        .retain(|e, _| alive_peers.contains(e));

    // Prune persistence for avatars that are no longer in either
    // query. We do NOT synthesise an Exit transition for a vanished
    // entity — consumers can't act on a sample for a despawned avatar
    // anyway.
    persistence
        .last_surface
        .retain(|e, _| alive_peers.contains(e) || locals.contains(*e));
}

/// Shared per-avatar emission path used by both the local-player and
/// remote-peer loops. Probes the surface registry, asks
/// [`compute_transitions`] for the right phase set, and pushes one
/// [`ContactSample`] per transition.
#[allow(clippy::too_many_arguments)]
fn emit_for_avatar(
    avatar: Entity,
    world_pos: Vec3,
    world_vel: Vec3,
    footprint_radius: f32,
    locomotion: &LocomotionConfig,
    water: &WaterSurfaces,
    persistence: &mut ContactPersistence,
    out: &mut Vec<ContactSample>,
) {
    let total_height = locomotion_total_height(locomotion);
    let last = persistence.last_surface.get(&avatar).copied();
    // Schmitt trigger: if we were already in water contact, probe with
    // the wide exit threshold so a settling bob can't chatter.
    let was_in_contact = matches!(last, Some(SurfaceContact::Water { .. }));
    let curr = probe_surface(world_pos, total_height, was_in_contact, water);
    let (transitions, new_state) = compute_transitions(last, curr);

    // TEMP DIAGNOSTIC #254 — remove once the settle-burst root cause is
    // confirmed. Gated on OVERLANDS_WAKE_DEBUG so default builds, tests
    // and normal runtime are byte-for-byte unaffected. Tests the
    // hypothesis that a pitching/heaving HoverBoat settle lifts the
    // axis-aligned body-bottom probe past CONTACT_EXIT_SLACK, flipping
    // contact Some<->None and shedding an Enter/Exit SplashRing burst.
    if std::env::var_os("OVERLANDS_WAKE_DEBUG").is_some() {
        let body_bottom = Vec3::new(world_pos.x, world_pos.y - 0.5 * total_height, world_pos.z);
        let signed_depth = water.query_signed(body_bottom).map(|q| q.depth);
        // Relevance gate: only emit near/at/leaving water or on any
        // transition. Boat-on-land-far-from-water frames stay silent so
        // every captured line is the contact/settle window.
        let near_water = signed_depth.is_some_and(|d| d > -3.0);
        if was_in_contact || curr.is_some() || !transitions.is_empty() || near_water {
            let slack = if was_in_contact {
                wcfg::CONTACT_EXIT_SLACK
            } else {
                wcfg::CONTACT_SLACK
            };
            let phases: Vec<ContactPhase> = transitions.iter().map(|t| t.phase).collect();
            info!(
                target: "wake_debug",
                "probe avatar={:?} origin_y={:.4} h={:.3} body_bottom_y={:.4} \
                 signed_depth={:?} was_in_contact={} slack={:.2} \
                 curr_some={} -> phases={:?}",
                avatar,
                world_pos.y,
                total_height,
                body_bottom.y,
                signed_depth.map(|d| (d * 1000.0).round() / 1000.0),
                was_in_contact,
                slack,
                curr.is_some(),
                phases,
            );
        }
    }

    for t in transitions {
        let intensity = intensity_for(&t.surface, locomotion);
        out.push(ContactSample {
            avatar,
            world_pos,
            world_vel,
            footprint_radius,
            surface: t.surface,
            intensity,
            phase: t.phase,
        });
    }

    match new_state {
        Some(s) => {
            persistence.last_surface.insert(avatar, s);
        }
        None => {
            persistence.last_surface.remove(&avatar);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water_contact(plane_idx: usize, depth: f32) -> SurfaceContact {
        SurfaceContact::Water {
            plane_idx,
            depth,
            flow_dir: Vec2::ZERO,
        }
    }

    /// A single flat water plane at y = 0 large enough that XZ is
    /// always inside its extents for these tests.
    fn flat_water() -> WaterSurfaces {
        use crate::water::WaterPlane;
        WaterSurfaces {
            planes: vec![WaterPlane {
                world_from_local: Transform::from_translation(Vec3::ZERO),
                local_half_extents: Vec2::splat(100.0),
                flow_strength: 0.0,
            }],
        }
    }

    /// Place `world_pos` so the probed body bottom sits exactly
    /// `above` metres above the y=0 surface (negative = submerged).
    fn pos_for_body_bottom(above: f32, total_height: f32) -> Vec3 {
        Vec3::new(0.0, above + 0.5 * total_height, 0.0)
    }

    #[test]
    fn contact_uses_hysteresis_to_resist_settling_chatter() {
        let water = flat_water();
        let h = 2.0;

        // Body bottom 0.3 m above the surface — beyond the tight
        // enter slack (0.15) but inside the wide exit slack (0.6).
        let just_above = pos_for_body_bottom(0.3, h);
        // Not yet in contact → tight threshold rejects it.
        assert!(probe_surface(just_above, h, false, &water).is_none());
        // Already in contact → hysteresis holds it (no Exit chatter).
        assert!(probe_surface(just_above, h, true, &water).is_some());

        // Clearly clear of the water (1.0 m > exit slack) → Exit even
        // with hysteresis: a genuine departure still ends contact.
        let clear = pos_for_body_bottom(1.0, h);
        assert!(probe_surface(clear, h, true, &water).is_none());

        // Submerged → in contact regardless of prior state.
        let under = pos_for_body_bottom(-0.1, h);
        assert!(probe_surface(under, h, false, &water).is_some());
        assert!(probe_surface(under, h, true, &water).is_some());
    }

    #[test]
    fn first_contact_emits_enter_only() {
        let curr = water_contact(0, 0.5);
        let (trans, state) = compute_transitions(None, Some(curr));
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Enter);
        assert_eq!(state, Some(curr));
    }

    #[test]
    fn same_surface_continued_emits_dwell() {
        let prev = water_contact(0, 0.3);
        let curr = water_contact(0, 0.5);
        let (trans, state) = compute_transitions(Some(prev), Some(curr));
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Dwell);
        // Dwell carries the *current* surface payload so depth changes
        // are visible to consumers.
        assert_eq!(trans[0].surface, curr);
        assert_eq!(state, Some(curr));
    }

    #[test]
    fn leaving_water_emits_exit_and_clears_state() {
        let prev = water_contact(0, 0.5);
        let (trans, state) = compute_transitions(Some(prev), None);
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Exit);
        assert_eq!(trans[0].surface, prev);
        assert_eq!(state, None);
    }

    #[test]
    fn switching_water_planes_emits_exit_then_enter() {
        let prev = water_contact(0, 0.4);
        let curr = water_contact(1, 0.6);
        let (trans, state) = compute_transitions(Some(prev), Some(curr));
        assert_eq!(trans.len(), 2);
        assert_eq!(trans[0].phase, ContactPhase::Exit);
        assert_eq!(trans[0].surface, prev);
        assert_eq!(trans[1].phase, ContactPhase::Enter);
        assert_eq!(trans[1].surface, curr);
        assert_eq!(state, Some(curr));
    }

    #[test]
    fn no_contact_no_history_emits_nothing() {
        let (trans, state) = compute_transitions(None, None);
        assert!(trans.is_empty());
        assert_eq!(state, None);
    }

    #[test]
    fn intensity_saturates_when_fully_submerged() {
        let cfg = LocomotionConfig::Humanoid(Box::new(crate::pds::HumanoidParams::default()));
        let height = locomotion_total_height(&cfg);
        let shallow = intensity_for(&water_contact(0, 0.1), &cfg);
        let mid = intensity_for(&water_contact(0, height * 0.5), &cfg);
        let submerged = intensity_for(&water_contact(0, height * 5.0), &cfg);
        assert!(shallow > 0.0 && shallow < 0.2);
        assert!((mid - 0.5).abs() < 1e-3);
        assert!((submerged - 1.0).abs() < 1e-6);
    }
}
