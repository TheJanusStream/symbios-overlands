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
//! `visuals` tree, which an attachment is not part of; the attachment editor
//! (#1059) gets its own selection story.

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
