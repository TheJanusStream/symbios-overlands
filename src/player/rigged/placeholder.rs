//! The stand-in the owner wears while their own body is not standing
//! (#1255).
//!
//! The rigged build is not one of the loading gate's tasks, and cannot
//! easily become one: `spawn_local_player` runs `OnEnter(AppState::InGame)`,
//! so during `Loading` there is no chassis to hang a build on and no
//! `RiggedBuild` for a gate row to watch. The build is kicked on the first
//! `InGame` frames instead, and on wasm that dispatch is the first use of
//! the gen-worker for this job kind - `offload.rs` records the bill: 839 KB
//! gzipped of avatar engine against ~16 KB before, in one lazy fetch, on top
//! of the worker's own 130 ms–1.0 s instantiation. `offload/worker.rs` states
//! the consequence in its own words: "until it lands the wearer is a bare
//! chassis - a person shaped like nothing, walking around a room."
//!
//! So the wait is made legible where it happens rather than moved. #1217
//! already settled what a body-shaped absence should look like - a
//! translucent capsule that cannot be mistaken for anybody - and built it
//! for peers; this is the same stand-in, the same mesh and the same numbers,
//! for the one chassis that was still allowed to be a void: your own.
//!
//! Two details are inherited from that work rather than rediscovered. The
//! mesh goes on the **chassis entity itself**, because
//! `spawn_avatar_visuals` clears every chassis child before it spawns -
//! including, for a rigged body, before spawning nothing at all - so a child
//! placeholder would be destroyed at exactly the moment it is most needed.
//! And one mesh and one material are cached for the session, so a body
//! rebuilt on every slider drag cannot leak an asset per attempt.
//!
//! A beast lying along its capsule (#1802) waits under a lying stand-in, its
//! own collider's size, rather than a person's upright one standing through
//! the ground: that mesh is cached too, one at a time, and replaced only when
//! the collider it was cut for changes.

use bevy::prelude::*;

use crate::config;
use crate::state::{LiveAvatarRecord, LocalPlayer};

use super::{RiggedBuild, RiggedRoot, SLOW_BUILD_ANNOUNCE_SECS};

/// The owner's chassis, its children, and the stand-in it is already
/// wearing, if it is - the one query [`sync_local_placeholder`] reconciles
/// over.
type LocalChassis<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Children>,
        Option<&'static LocalPlaceholder>,
    ),
    With<LocalPlayer>,
>;

/// The local chassis is currently wearing the stand-in, of this shape.
///
/// Visible as far as the system that uses it, because it appears in
/// [`sync_local_placeholder`]'s query and that is registered from
/// `player::PlayerPlugin`.
#[derive(Component, Clone, Copy, PartialEq)]
pub(in crate::player) struct LocalPlaceholder(StandIn);

/// Which stand-in a body waits under (#1802): the person-sized one, or one
/// lying along a beast's collider, by its radius and cylindrical length (m)
/// and how far ahead of the body's origin its middle lies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum StandIn {
    Upright,
    Lying {
        radius: f32,
        length: f32,
        ahead: f32,
    },
}

impl StandIn {
    /// The stand-in for a body wearing `live`: lying along a beast's capsule
    /// as its chassis builds it, upright for every other body.
    pub(crate) fn of(live: &crate::pds::AvatarRecord) -> Self {
        match &live.locomotion {
            crate::pds::LocomotionConfig::Humanoid(p) if p.is_lying() => Self::Lying {
                radius: p.capsule_radius.0.max(0.05),
                length: p.capsule_length.0.max(0.1),
                ahead: p.capsule_ahead(),
            },
            _ => Self::Upright,
        }
    }

    pub(crate) fn mesh(self) -> Mesh {
        match self {
            Self::Upright => Capsule3d::new(
                config::network::BODY_PLACEHOLDER_RADIUS,
                config::network::BODY_PLACEHOLDER_LENGTH,
            )
            .into(),
            // Avian's lying capsule runs along the body's Z, its middle ahead
            // toward -Z, Bevy's forward; Bevy's capsule along Y.
            Self::Lying {
                radius,
                length,
                ahead,
            } => Mesh::from(Capsule3d::new(radius, length))
                .rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .translated_by(Vec3::NEG_Z * ahead),
        }
    }
}

/// The stand-in's assets, kept for the session: the one material, the
/// upright mesh, and the lying mesh last cut, with the shape it was cut for.
#[derive(Default)]
pub(in crate::player) struct StandInAssets {
    material: Option<Handle<StandardMaterial>>,
    upright: Option<Handle<Mesh>>,
    lying: Option<(StandIn, Handle<Mesh>)>,
}

/// Put the stand-in on the owner's chassis whenever their rigged body is not
/// standing, and take it off the moment one is.
///
/// Deliberately a reconciler with no latch, unlike the peer pair it mirrors.
/// A peer arrives once; the owner's body is rebuilt on every settled slider
/// edit, and can *fail* to build - [`RiggedBuild`] is then removed with no
/// root installed. A latched "retired" state would leave that case a void
/// again, which is the exact failure this exists to end. The question asked
/// each frame is therefore the honest one: does a body stand here?
///
/// A rebuild does not flicker the stand-in on, because `install_built_body`
/// despawns the old root only once the new one is ready: a body that is
/// being replaced is still standing throughout.
pub(in crate::player) fn sync_local_placeholder(
    mut commands: Commands,
    live: Option<Res<LiveAvatarRecord>>,
    locals: LocalChassis,
    roots: Query<(), With<RiggedRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cached: Local<StandInAssets>,
) {
    for (chassis, children, wearing) in &locals {
        // Only a RIGGED record can leave the chassis empty. A generator body
        // spawns its tree synchronously in `spawn_avatar_visuals`, and a
        // record that names neither is a bare chassis by contract (#1217) -
        // dressing that one would be inventing a body the owner declined.
        let rigged = live
            .as_ref()
            .is_some_and(|live| live.0.body.rigged_ref().is_some());
        let standing =
            children.is_some_and(|children| children.iter().any(|child| roots.contains(child)));
        let wants = (rigged && !standing).then(|| {
            live.as_ref()
                .map_or(StandIn::Upright, |live| StandIn::of(&live.0))
        });
        if wants == wearing.map(|worn| worn.0) {
            continue;
        }
        if let Some(stand_in) = wants {
            let material = cached
                .material
                .get_or_insert_with(|| {
                    materials.add(StandardMaterial {
                        base_color: config::network::BODY_PLACEHOLDER_COLOR,
                        alpha_mode: AlphaMode::Blend,
                        perceptual_roughness: 1.0,
                        ..default()
                    })
                })
                .clone();
            let mesh = match stand_in {
                StandIn::Upright => cached
                    .upright
                    .get_or_insert_with(|| meshes.add(stand_in.mesh()))
                    .clone(),
                StandIn::Lying { .. } => match &cached.lying {
                    Some((cut, mesh)) if *cut == stand_in => mesh.clone(),
                    _ => {
                        let mesh = meshes.add(stand_in.mesh());
                        cached.lying = Some((stand_in, mesh.clone()));
                        mesh
                    }
                },
            };
            commands.entity(chassis).insert((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                LocalPlaceholder(stand_in),
            ));
        } else {
            commands
                .entity(chassis)
                .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>, LocalPlaceholder)>();
        }
    }
}

/// What the owner is told when their own body is taking a visible while.
///
/// Names the cost rather than the mechanism: the first build of a session on
/// the web pays a one-off engine download, and "a moment the first time" is
/// the true and useful half of that. A build that finishes at the usual
/// ~277 ms never reaches this line.
pub(in crate::player) const SLOW_BUILD_LINE: &str =
    "Building your body - the first one takes a moment.";

/// Say so when the owner's build has been in flight long enough to notice.
///
/// The stand-in above answers "am I here?"; this answers "is anything
/// happening?". Without it a gen-worker that 404s is indistinguishable from
/// a slow one until the offload watchdog notices at 60 seconds, and then
/// only in the diagnostics log.
///
/// Announced once per build: the flag lives on [`RiggedBuild`], so it is
/// destroyed with the build it describes and a retry gets to speak again.
/// Peers are excluded - a stranger's slow body is not the owner's problem,
/// and a busy room would toast per arrival.
pub(in crate::player) fn announce_slow_builds(
    time: Res<Time>,
    mut builds: Query<&mut RiggedBuild, With<LocalPlayer>>,
    mut toasts: Option<ResMut<crate::notify::Toasts>>,
) {
    let now = time.elapsed_secs_f64();
    for mut build in &mut builds {
        if build.announced || now - build.kicked_at < SLOW_BUILD_ANNOUNCE_SECS {
            continue;
        }
        build.announced = true;
        if let Some(toasts) = toasts.as_deref_mut() {
            toasts.info(SLOW_BUILD_LINE, now);
        }
    }
}
