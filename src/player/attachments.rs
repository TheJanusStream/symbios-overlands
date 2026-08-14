//! Worn props on rigged bodies (#1058, epic #1054).
//!
//! An attachment is an overlands `Generator` tree pinned to a rig socket:
//! the record half is [`crate::pds::AttachmentRecord`] (an owned copy of the
//! item, the socket's stable name, a quantised offset), resolved alongside
//! the avatar record; this module is the runtime half. Attachment is nothing
//! more than parenting — the sibling crate spawns a real entity per rig
//! joint ([`AvatarJoints`]), so a prop rides its joint through every clip
//! and procedural gait for free, and **never** touches
//! `symbios_avatar::Rig::attach`, which after a build would desync the baked
//! inverse bindposes (the trap the engine's socket docs carry).
//!
//! Placement is graded exactly like resolution is:
//!
//!   - a socket name this build does not know → the prop is not worn (the
//!     record keeps it for the client that does);
//!   - a socket this body does not have (a tail prop on a biped) → not worn;
//!   - an **identity** offset → the engine seats the prop itself:
//!     [`symbios_avatar::Socket::seat`] pushes the socket's anchor outside
//!     the *measured* surface, so a first attach lands visible on every
//!     body instead of embedded in a chest ([`SEAT_MARGIN`] of air);
//!   - an authored offset → taken verbatim, in the joint's rest-pose frame,
//!     with the uniform scale the record's sanitiser enforced.
//!
//! Props spawn through [`spawn_visual_tree`] — the same avatar-mode pipeline
//! as generator bodies, colliders and room tags suppressed — always with
//! `is_local = false`: `AvatarVisualPrim` paths index into a record's own
//! `visuals` tree, which an attachment is not part of. The owner's own props
//! instead carry [`LocalAttachment`], the identity the numeric editor
//! (#1059) and the in-world offset gizmo (#1062) address them by.

use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarJoints};

use super::rigged::RiggedRoot;
use super::visuals::{AvatarSpawnDeps, spawn_visual_tree};
use crate::pds::avatar::ResolvedAttachment;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};

/// Air left between a default-seated prop's origin and the skin, in metres.
const SEAT_MARGIN: f32 = 0.02;

/// The root entity of one worn prop, a child of its rig joint's entity.
#[derive(Component)]
pub(super) struct AttachmentRoot;

/// Editor identity for one of the **local** player's worn props (#1062).
///
/// Only the local body's props carry it — exactly as `AvatarVisualPrim` only
/// rides local visuals — so a scene pick or a gizmo can never reach into a
/// peer's outfit. It carries everything the in-world offset editor needs
/// that the entity itself cannot answer:
///
///   - `rkey` — the record half of the `(rkey, socket)` pair an attachment
///     is addressed by. A worn prop is *not* a node in any visuals tree, so
///     an `AvatarVisualPrim` path cannot name one.
///   - `joint` + `rigged_root` — the carrying joint's index into
///     `Avatar::rig` and the body root it hangs off, which together give the
///     joint's **rest** frame in world space. The stored offset lives in
///     that frame, not in the animated one the joint entity is at.
#[derive(Component, Clone, Debug)]
pub(crate) struct LocalAttachment {
    /// Record key of the attachment record this prop was spawned from.
    pub(crate) rkey: String,
    /// Index of the carrying joint into the built body's `rig.joints`.
    pub(crate) joint: usize,
    /// The [`RiggedRoot`] the joint hierarchy hangs off — the entity whose
    /// `GlobalTransform` places the rig's rest frame in the world.
    pub(crate) rigged_root: Entity,
}

impl LocalAttachment {
    /// Where the carrying joint sits **at rest**, in world space.
    ///
    /// The engine spawns joints at the bind pose — every joint unrotated at
    /// its rig position (see `bevy_symbios_avatar::spawn_joints`) — so the
    /// rest frame is the body root's frame translated by the joint's rig
    /// position, with no rotation of its own. This is the frame an
    /// attachment offset is authored and stored in; the joint entity's live
    /// `GlobalTransform` is the *animated* frame and drifts away from it
    /// every frame a clip plays.
    pub(crate) fn rest_frame(
        &self,
        avatar: &symbios_avatar::Avatar,
        root_world: &GlobalTransform,
    ) -> Option<GlobalTransform> {
        let joint = avatar.rig.joints.get(self.joint)?;
        Some(root_world.mul_transform(Transform::from_translation(joint.position)))
    }
}

/// What is currently worn on this rigged body, kept on the [`RiggedRoot`]
/// entity — deliberately, because a body rebuild replaces that entity and a
/// fresh root therefore re-dresses from the record without any bookkeeping.
#[derive(Component, Default)]
pub(super) struct AttachmentsApplied {
    applied: Vec<ResolvedAttachment>,
    spawned: Vec<Entity>,
}

/// Dress every rigged body whose worn set differs from its record's.
///
/// Runs after [`super::rigged::land_rigged_builds`] in the Build set, so a
/// body landed this frame is dressed this frame. Both directions of change
/// are one path: despawn what was worn, spawn what the record says now.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn sync_rigged_attachments(
    mut commands: Commands,
    live: Option<Res<LiveAvatarRecord>>,
    locals: Query<(), With<LocalPlayer>>,
    peers: Query<&RemotePeer>,
    mut roots: Query<
        (
            Entity,
            &ChildOf,
            &BuiltBody,
            &AvatarJoints,
            Option<&mut AttachmentsApplied>,
        ),
        With<RiggedRoot>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
) {
    for (root, child_of, body, joints, mut applied) in &mut roots {
        let chassis = child_of.parent();
        // Whose record dresses this body: the local player's live record, or
        // the peer's fetched one. A chassis that is neither (mid-despawn)
        // keeps whatever it wears until it goes.
        let empty: &[ResolvedAttachment] = &[];
        let desired: &[ResolvedAttachment] = if locals.contains(chassis) {
            live.as_ref()
                .and_then(|live| live.0.body.rigged_ref())
                .and_then(|rig| rig.resolved.as_ref())
                .map_or(empty, |resolved| &resolved.attachments)
        } else if let Ok(peer) = peers.get(chassis) {
            peer.avatar
                .as_ref()
                .and_then(|record| record.body.rigged_ref())
                .and_then(|rig| rig.resolved.as_ref())
                .map_or(empty, |resolved| &resolved.attachments)
        } else {
            continue;
        };

        if applied.as_ref().is_some_and(|worn| worn.applied == desired) {
            continue;
        }
        if let Some(worn) = applied.as_mut() {
            for &prop in &worn.spawned {
                commands.entity(prop).despawn();
            }
        }

        let is_local = locals.contains(chassis);
        let mut spawned = Vec::new();
        for (joint, transform, attachment) in placements(&body.avatar, desired) {
            let Some(&carrier) = joints.0.get(joint) else {
                continue;
            };
            let prop = commands
                .spawn((
                    AttachmentRoot,
                    transform,
                    Visibility::default(),
                    ChildOf(carrier),
                ))
                .id();
            // Only the owner's own props are editable, so only they carry
            // the editor identity (#1062) — a peer's outfit can then never
            // be picked or dragged, the same invariant `AvatarVisualPrim`
            // holds for visuals.
            if is_local {
                commands.entity(prop).insert(LocalAttachment {
                    rkey: attachment.rkey.clone(),
                    joint,
                    rigged_root: root,
                });
            }
            spawn_visual_tree(
                &mut commands,
                prop,
                &attachment.record.item,
                &mut meshes,
                &mut materials,
                &mut images,
                &mut deps,
                false,
            );
            spawned.push(prop);
        }
        commands.entity(root).insert(AttachmentsApplied {
            applied: desired.to_vec(),
            spawned,
        });
    }
}

/// Which joint carries each wearable attachment, and at what local
/// transform. Pure — the whole placement decision, kept apart from the ECS
/// so it can be tested against a real build without a world.
fn placements<'a>(
    avatar: &symbios_avatar::Avatar,
    desired: &'a [ResolvedAttachment],
) -> Vec<(usize, Transform, &'a ResolvedAttachment)> {
    let mut out = Vec::new();
    for attachment in desired {
        let Some(socket) = attachment.record.socket() else {
            info!(
                "attachment {} names socket '{}' this build does not know — not worn",
                attachment.rkey, attachment.record.socket
            );
            continue;
        };
        let Some(joint) = socket.joint(&avatar.rig) else {
            // A tail prop on a biped: the record is honoured by the body
            // that has the part.
            continue;
        };
        let transform = if attachment.record.offset.is_identity() {
            seated_default(socket, avatar, joint)
        } else {
            transform_from_data(&attachment.record.offset)
        };
        out.push((joint, transform, attachment));
    }
    out
}

/// The engine-seated default placement: the socket's anchor pushed outside
/// the measured surface, expressed in the carrying joint's rest frame —
/// which is the frame the joint entity's children live in.
fn seated_default(
    socket: symbios_avatar::Socket,
    avatar: &symbios_avatar::Avatar,
    joint: usize,
) -> Transform {
    match socket.seat(&avatar.rig, &avatar.parts.surface, SEAT_MARGIN) {
        Some(anchor) => {
            Transform::from_translation(anchor.position - avatar.rig.joints[joint].position)
        }
        None => Transform::default(),
    }
}

/// PDS `TransformData` → Bevy `Transform`. The same three lines
/// `world_builder::avatar_spawn` keeps for itself; the compile module's
/// helper is `pub(super)` there.
fn transform_from_data(t: &crate::pds::TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::wardrobe::{AttachmentRecord, engine_default_for_did};
    use crate::pds::types::Fp3;
    use crate::pds::{Generator, TransformData};

    fn built() -> symbios_avatar::Avatar {
        symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:attachment-test"),
            &symbios_avatar::AvatarConfig {
                atlas: 128,
                ..Default::default()
            },
        )
        .expect("the seeded default engine body builds")
    }

    fn worn(record: AttachmentRecord) -> ResolvedAttachment {
        ResolvedAttachment {
            rkey: String::from("3jzfcijpj2z2a"),
            record,
        }
    }

    #[test]
    fn placement_seats_identity_offsets_and_honours_authored_ones() {
        let avatar = built();

        // Identity offset: the engine seats the prop outside the measured
        // surface. A crown prop must land above the head joint, not at it.
        let crown = worn(AttachmentRecord::new(
            Generator::default(),
            symbios_avatar::Socket::Crown,
        ));
        // Authored offset: taken verbatim in the joint's frame.
        let mut hand_record =
            AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::LeftHand);
        hand_record.offset = TransformData {
            translation: Fp3([0.1, 0.2, 0.3]),
            ..Default::default()
        };
        let hand = worn(hand_record);
        // A socket from a future client is kept but not worn.
        let mut alien_record =
            AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Chest);
        alien_record.socket = String::from("third-elbow");
        let alien = worn(alien_record);
        // A tail prop on a biped resolves to no joint and is not worn.
        let tail = worn(AttachmentRecord::new(
            Generator::default(),
            symbios_avatar::Socket::Tail,
        ));

        let outfit = [crown, hand, alien, tail];
        let placed = placements(&avatar, &outfit);
        assert_eq!(placed.len(), 2, "two of four props are wearable here");

        let (crown_joint, crown_tf, _) = &placed[0];
        assert_eq!(
            symbios_avatar::Socket::Crown.joint(&avatar.rig),
            Some(*crown_joint)
        );
        assert!(
            crown_tf.translation.y > 0.0,
            "a seated crown prop sits above its joint, got {}",
            crown_tf.translation.y
        );

        let (hand_joint, hand_tf, _) = &placed[1];
        assert_eq!(
            symbios_avatar::Socket::LeftHand.joint(&avatar.rig),
            Some(*hand_joint)
        );
        assert!((hand_tf.translation - Vec3::new(0.1, 0.2, 0.3)).length() < 1e-6);
    }

    /// The whole in-world offset editor (#1062) rests on one cross-crate
    /// claim: the frame an attachment offset is stored in is the *bind
    /// pose*, and the bind pose puts every joint entity unrotated at its rig
    /// position. This checks that against the sibling crate's real spawn and
    /// pose systems rather than against a re-derivation of them — if the
    /// engine ever poses a rest body differently, every committed drag
    /// silently lands somewhere else, and this is the test that says so.
    #[test]
    fn the_rest_frame_is_where_the_engine_actually_puts_a_joint_at_rest() {
        use bevy::MinimalPlugins;
        use bevy::asset::AssetPlugin;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
        use bevy::transform::TransformPlugin;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<Image>();
        app.init_asset::<SkinnedMeshInverseBindposes>();

        // A body somewhere other than the origin, turned: a rest frame that
        // only happens to be right at the identity would pass a weaker test.
        let root_tf =
            Transform::from_xyz(11.0, -0.75, -4.25).with_rotation(Quat::from_rotation_y(1.1));
        let root = app.world_mut().spawn((root_tf, Visibility::default())).id();
        let mut built = Some(built());
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    // `Avatar` withholds `Clone` deliberately, so the single
                    // build is taken out of an `Option` on the one run.
                    let Some(avatar) = built.take() else {
                        return;
                    };
                    bevy_symbios_avatar::spawn_avatar(
                        &mut commands,
                        root,
                        avatar,
                        0.0,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    );
                },
            )
            .expect("spawns");

        // Pose it at rest through the engine's own writer, then propagate.
        let rest_pose = {
            let body = app
                .world()
                .get::<BuiltBody>(root)
                .expect("the body landed on the root");
            symbios_avatar::Pose::rest(&body.avatar.rig)
        };
        app.world_mut()
            .entity_mut(root)
            .insert(bevy_symbios_avatar::AvatarPose(rest_pose));
        app.world_mut()
            .run_system_once(bevy_symbios_avatar::spawn::apply_avatar_poses)
            .expect("poses");
        app.update();

        let joints = app
            .world()
            .get::<AvatarJoints>(root)
            .expect("joints")
            .0
            .clone();
        let body = app.world().get::<BuiltBody>(root).expect("body");
        let root_world = *app
            .world()
            .get::<GlobalTransform>(root)
            .expect("root global");

        for socket in [
            symbios_avatar::Socket::Crown,
            symbios_avatar::Socket::Chest,
            symbios_avatar::Socket::LeftHand,
            symbios_avatar::Socket::RightFoot,
        ] {
            let Some(joint) = socket.joint(&body.avatar.rig) else {
                continue;
            };
            let worn = LocalAttachment {
                rkey: String::from("3jzfcijpj2z2a"),
                joint,
                rigged_root: root,
            };
            let claimed = worn
                .rest_frame(&body.avatar, &root_world)
                .expect("the joint is in this rig");
            let actual = *app
                .world()
                .get::<GlobalTransform>(joints[joint])
                .expect("joint global");
            let (claimed, actual) = (claimed.compute_transform(), actual.compute_transform());
            assert!(
                claimed.translation.distance(actual.translation) < 1e-4,
                "{socket:?}: rest frame at {} but the engine posed the joint at {}",
                claimed.translation,
                actual.translation
            );
            assert!(
                claimed.rotation.angle_between(actual.rotation) < 1e-3,
                "{socket:?}: a rest joint is unrotated; the engine gave {:?}",
                actual.rotation
            );
        }
    }

    /// A released world pose converts back to the stored offset through the
    /// rest frame, and doing it through the *animated* joint instead — the
    /// obvious-looking shortcut, and what `resolve_committed_local` would do
    /// — gets a materially different answer. That difference is the bug the
    /// rest frame exists to prevent, so it is asserted rather than implied.
    #[test]
    fn an_offset_round_trips_through_the_rest_frame_but_not_an_animated_one() {
        let avatar = built();
        let joint = symbios_avatar::Socket::LeftHand
            .joint(&avatar.rig)
            .expect("a biped has a left hand");
        let worn = LocalAttachment {
            rkey: String::from("3jzfcijpj2z2a"),
            joint,
            rigged_root: Entity::PLACEHOLDER,
        };
        let root_world = GlobalTransform::from(
            Transform::from_xyz(-3.0, 0.4, 8.0).with_rotation(Quat::from_rotation_y(-0.7)),
        );
        let rest = worn
            .rest_frame(&avatar, &root_world)
            .expect("the joint is in this rig");

        let offset = Transform::from_xyz(0.06, -0.11, 0.03)
            .with_rotation(Quat::from_rotation_x(0.5))
            .with_scale(Vec3::splat(1.25));
        let released = rest.mul_transform(offset);

        let back = released.reparented_to(&rest);
        assert!((back.translation - offset.translation).length() < 1e-5);
        assert!(back.rotation.angle_between(offset.rotation) < 1e-4);
        assert!((back.scale - offset.scale).length() < 1e-5);

        // Mid-swing, the same world pose read against the live joint frame.
        let swung = rest.mul_transform(Transform::from_rotation(Quat::from_rotation_z(0.9)));
        let wrong = released.reparented_to(&swung);
        assert!(
            wrong.translation.distance(offset.translation) > 0.02,
            "an animated-frame commit must not accidentally agree; got {} vs {}",
            wrong.translation,
            offset.translation
        );
    }

    #[test]
    fn a_seated_prop_stands_clear_of_the_measured_body() {
        // The whole reason seat() is used over the raw anchor: a chest prop
        // whose default placement is inside the ribcage is invisible, and
        // nobody's first attach should need the gizmo to find.
        let avatar = built();
        for socket in [
            symbios_avatar::Socket::Chest,
            symbios_avatar::Socket::Back,
            symbios_avatar::Socket::Waist,
        ] {
            let attachment = worn(AttachmentRecord::new(Generator::default(), socket));
            let placed = placements(&avatar, std::slice::from_ref(&attachment));
            let (joint, transform, _) = placed.first().expect("wearable");
            let world = avatar.rig.joints[*joint].position + transform.translation;
            let clearance = avatar
                .parts
                .surface
                .clearance(&avatar.rig, world, SEAT_MARGIN * 0.5);
            assert_eq!(
                clearance,
                Vec3::ZERO,
                "{socket:?} seated inside the body, still owing {clearance}"
            );
        }
    }
}
