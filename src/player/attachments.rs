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
//!   - an identity offset **with a fit declaration** (#1089) → the prop is
//!     seated on the measured body itself: origin on the head axis at the
//!     measured hat line, subtree scaled uniformly so the declared band
//!     diameter matches the wearer's brow circumference ([`fitted_seat`],
//!     [`hat_line`]); an unmeasurable head falls back to the plain seat at
//!     authored size;
//!   - an authored offset → taken verbatim, in the joint's rest-pose frame:
//!     a full transform, per-axis scale included (#1095).
//!
//! Props spawn through [`spawn_attachment_tree`] — the same avatar-mode
//! pipeline as generator bodies, colliders and room tags suppressed — always
//! with `is_local = false`: `AvatarVisualPrim` paths index into a record's
//! own `visuals` tree, which an attachment is not part of. The owner's own
//! props instead carry [`LocalAttachment`] on their root, the identity the
//! numeric editor (#1059) and the in-world offset gizmo (#1062) address them
//! by, and an `AttachmentPrim` on every node (#1098) so their PARTS can be
//! edited like a region asset's.

use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarJoints};

use super::rigged::RiggedRoot;
use super::visuals::{AvatarSpawnDeps, spawn_attachment_tree};
use crate::pds::avatar::ResolvedAttachment;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};

/// Air left between a default-seated prop's origin and the skin, in metres.
const SEAT_MARGIN: f32 = 0.02;

/// Height samples walked between the eye line and the dome cap when hunting
/// the hat line ([`hat_line`]). The vault's perimeter varies
/// slowly in height — [`symbios_avatar::face::Skull`] itself holds a
/// handful of bands — so a dense scan buys nothing.
const HAT_LINE_STEPS: usize = 16;

/// Azimuth samples the perimeter polygon is summed over. At 64 the chord
/// shortfall of a circle is under 0.1%, far inside the wire's own
/// millimetre quantisation.
const HAT_LINE_AZIMUTHS: usize = 64;

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
    /// The record's inventory provenance (#1097), so the scene menu can
    /// name the prop and offer Save-to-inventory without a record lookup.
    pub(crate) source: Option<String>,
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
///
/// Per prop (#1104): each worn record beside the entity it was spawned as,
/// so an edit to one prop replaces that prop alone and the rest of the
/// outfit stands untouched.
#[derive(Component, Default)]
pub(super) struct AttachmentsApplied {
    worn: Vec<(ResolvedAttachment, Entity)>,
}

/// Latch marking a rigged root as dressed exactly as its record says (#1135).
///
/// Sibling of `rigged::RiggedSteady`, and for the same reason: the compare it
/// guards is a value-equality over the full `ResolvedAttachment` list, which
/// carries every worn prop's `AttachmentRecord` `Generator` tree. A room of
/// peers in elaborate outfits paid that per body per frame to conclude
/// "unchanged" for thousands of consecutive frames.
///
/// A latch and not a `Changed<>` gate because the answer depends on more than
/// the record: props are also despawned by the orphan sweeps at the top of
/// this system, so the sweeps clear every latch on any frame they fire and
/// the whole set is re-derived from scratch. One frame of full work after a
/// sweep, which happens during outfit editing, is not worth being clever
/// about.
#[derive(Component)]
pub(super) struct AttachmentsSteady;

impl AttachmentsApplied {
    /// The prop entities currently worn, in record order.
    #[cfg(test)]
    pub(super) fn spawned(&self) -> Vec<Entity> {
        self.worn.iter().map(|(_, prop)| *prop).collect()
    }
}

/// What `record` says is worn, or `None` when it cannot say yet.
///
/// The three answers are distinct and the middle one is the point (#1112):
///
///   - `Some(&[…])` — a rigged body with its references resolved: dress
///     exactly this.
///   - `None` — a rigged body whose references have not resolved (a
///     live-preview broadcast carries rkeys only; `resolved` never rides the
///     wire). Nothing is known, so nothing changes: keep what is standing
///     until the resolution lands.
///   - `Some(&[])` — a generator body, which wears no rig attachments at
///     all. Genuinely empty, so anything standing comes off.
fn dressed_by(record: &crate::pds::AvatarRecord) -> Option<&[ResolvedAttachment]> {
    match record.body.rigged_ref() {
        Some(rig) => rig
            .resolved
            .as_ref()
            .map(|resolved| resolved.attachments.as_slice()),
        None => Some(&[]),
    }
}

/// Dress every rigged body whose worn set differs from its record's.
///
/// Runs after [`super::rigged::land_rigged_builds`] in the Build set, so a
/// body landed this frame is dressed this frame. A per-prop diff (#1104):
/// a worn record equal by value to one already standing keeps its entity;
/// anything the record no longer says is despawned; anything new — or
/// changed, since a changed record is a new one — is spawned. Whole-outfit
/// replacement was the old behaviour and made every offset nudge blink the
/// entire loadout.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn sync_rigged_attachments(
    mut commands: Commands,
    live: Option<Res<LiveAvatarRecord>>,
    locals: Query<(), With<LocalPlayer>>,
    peers: Query<Ref<RemotePeer>>,
    dressed: Query<(), With<AttachmentsSteady>>,
    // Props orphaned from every hierarchy (#1077). Arming the in-world gizmo
    // on a worn prop deliberately detaches it from its joint — that is how
    // the gizmo renders a world pose mid-drag — and a body REBUILD landing
    // while it is armed despawns the root, the joints and every parented
    // prop, but not this one: with no `ChildOf` it survives the cascade,
    // the fresh root re-dresses a duplicate, and no ledger anywhere holds
    // the survivor. Detaching then removes the tracked prop and leaves the
    // phantom floating, which is exactly how the defect was found.
    //
    // The prop currently UNDER the gizmo is exempt (`Without<GizmoTarget>`):
    // parentless is its working state, and the release path drops its
    // markers — including against a parent that no longer exists — which
    // hands it to this sweep one frame later. The generator-visual twin of
    // this sweep is `despawn_orphan_avatar_visuals`, which cannot cover
    // props because a prop never carries `AvatarVisualPrim`.
    orphans: Query<
        Entity,
        (
            With<AttachmentRoot>,
            Without<ChildOf>,
            Without<transform_gizmo_bevy::GizmoTarget>,
        ),
    >,
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
    // Parts of worn props that hang off nothing (#1107). A part under the
    // gizmo is detached from its prop exactly as a prop is detached from its
    // joint (#1077), so replacing that prop — any record edit to it, a
    // material change from the parts editor included — despawns the prop
    // and every PARENTED node while the floating part survives as a ghost
    // of the old geometry. Two answers: the replacement below despawns the
    // record's own parentless parts in the same frame, and this sweep
    // catches any other path — a part whose detached-from parent no longer
    // exists cannot be committed (`resolve_committed_local` needs that
    // parent's pose), so it is a ghost whether or not a gizmo still holds
    // it. A live drag keeps its part: detached, but from a parent that is
    // still there.
    part_orphans: Query<
        (
            Entity,
            &crate::world_builder::AttachmentPrim,
            Option<&crate::editor_gizmo::GizmoDetachedPrim>,
        ),
        Without<ChildOf>,
    >,
    part_parents: Query<
        (),
        Or<(
            With<AttachmentRoot>,
            With<crate::world_builder::AttachmentPrim>,
        )>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
) {
    // Any sweep below removes a prop from under the record's feet, so the
    // dressed latches stop describing the world and every root is re-derived
    // this frame (#1135).
    let mut swept = false;
    for (ghost, _, detached) in &part_orphans {
        let drag_is_live = detached.is_some_and(|d| part_parents.contains(d.original_parent));
        if drag_is_live {
            continue;
        }
        swept = true;
        commands.entity(ghost).despawn();
        if let Some(metrics) = deps.caches.metrics.as_deref_mut() {
            crate::diagnostics::samplers::attachment_orphan_swept(metrics);
        }
    }
    for orphan in &orphans {
        swept = true;
        commands.entity(orphan).despawn();
        // Routed through the caches bundle (#924): this system already
        // carries `GeneratorCaches` inside `AvatarSpawnDeps`, and a sibling
        // `ResMut<MetricsRegistry>` beside it is a B0002 aliasing panic at
        // schedule build. Routine in small numbers during outfit editing
        // (#1077's gizmo-across-a-rebuild); growth outside an editing
        // session means a new path is orphaning props.
        if let Some(metrics) = deps.caches.metrics.as_deref_mut() {
            crate::diagnostics::samplers::attachment_orphan_swept(metrics);
        }
    }
    for (root, child_of, body, joints, mut applied) in &mut roots {
        let chassis = child_of.parent();
        // Whose record dresses this body: the local player's live record, or
        // the peer's fetched one. A chassis that is neither (mid-despawn)
        // keeps whatever it wears until it goes.
        let (desired, source_changed) = if locals.contains(chassis) {
            match live.as_ref() {
                Some(live) => (Some(dressed_by(&live.0)), live.is_changed()),
                None => (None, false),
            }
        } else if let Ok(peer) = peers.get(chassis) {
            let changed = peer.is_changed();
            (peer.into_inner().avatar.as_ref().map(dressed_by), changed)
        } else {
            continue;
        };
        // See `AttachmentsSteady`: skip the whole-outfit value comparison
        // below while nothing that feeds it can have moved.
        if source_changed || swept {
            commands.entity(root).remove::<AttachmentsSteady>();
        } else if dressed.contains(root) {
            continue;
        }
        // `None` is a WAIT, not an empty outfit (#1112) — the same rule
        // `kick_rigged_builds` applies to the body itself. A rigged record
        // whose references have not resolved yet cannot say what is worn,
        // and reading that as "wearing nothing" stripped every prop on
        // arrival of each live-preview broadcast and re-dressed it a
        // round-trip later, re-running the fitted props' measurements.
        let Some(Some(desired)) = desired else {
            continue;
        };

        // Compared through a borrowed slice rather than a collected `Vec` of
        // references: this ran per body per frame and the allocation was pure
        // churn on a heap that, on wasm, never shrinks (#1135).
        let already_worn: &[(ResolvedAttachment, Entity)] =
            applied.as_ref().map(|w| w.worn.as_slice()).unwrap_or(&[]);
        if already_worn.len() == desired.len()
            && already_worn.iter().zip(desired).all(|((a, _), b)| a == b)
        {
            // Dressed as described — latch it so the comparison is skipped
            // until the record changes or a sweep fires.
            commands.entity(root).insert(AttachmentsSteady);
            continue;
        }
        // Keep every standing prop the record still describes verbatim;
        // despawn the rest. A record edit changes the value, so an edited
        // prop is "gone" here and "new" below — replaced, alone.
        let mut kept: Vec<(ResolvedAttachment, Entity)> = Vec::new();
        if let Some(worn) = applied.as_mut() {
            for (record, prop) in worn.worn.drain(..) {
                if desired.contains(&record) && !kept.iter().any(|(k, _)| *k == record) {
                    kept.push((record, prop));
                } else {
                    commands.entity(prop).despawn();
                    // Its gizmo-detached parts are not in the cascade
                    // (#1107): take them with it, this frame, so the old
                    // geometry never draws beside the new.
                    for (ghost, marker, _) in &part_orphans {
                        if marker.rkey == record.rkey {
                            commands.entity(ghost).despawn();
                        }
                    }
                }
            }
        }
        let to_spawn: Vec<ResolvedAttachment> = desired
            .iter()
            .filter(|attachment| !kept.iter().any(|(k, _)| k == *attachment))
            .cloned()
            .collect();

        let is_local = locals.contains(chassis);
        let mut spawned = kept;
        ensure_joint_visibility(&mut commands, joints);
        for (joint, transform, attachment) in placements(&body.avatar, &to_spawn) {
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
                    source: attachment.record.source.clone(),
                });
            }
            // `is_local = false` always (a prop is not a visuals-tree
            // node); the owner's own props get `AttachmentPrim` part
            // markers instead (#1098), keyed by their record.
            spawn_attachment_tree(
                &mut commands,
                prop,
                &attachment.record.item,
                &mut meshes,
                &mut materials,
                &mut images,
                &mut deps,
                false,
                is_local.then_some(attachment.rkey.as_str()),
            );
            spawned.push((attachment.clone(), prop));
        }
        // Record order, so the equality fast-path above compares like with
        // like next frame.
        spawned.sort_by_key(|(record, _)| {
            desired
                .iter()
                .position(|d| d == record)
                .unwrap_or(usize::MAX)
        });
        commands
            .entity(root)
            .insert(AttachmentsApplied { worn: spawned });
    }
}

/// Which joint carries each wearable attachment, and at what local
/// transform. Pure — the whole placement decision, kept apart from the ECS
/// so it can be tested against a real build without a world.
pub(crate) fn placements<'a>(
    avatar: &symbios_avatar::Avatar,
    desired: &'a [ResolvedAttachment],
) -> Vec<(usize, Transform, &'a ResolvedAttachment)> {
    let mut out = Vec::new();
    // One head measurement dresses the whole outfit: `Skull::measure` walks
    // the full body mesh, so it runs at most once per dress, and only when
    // something worn actually declares a fit.
    let mut measured: Option<Option<HatLine>> = None;
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
            let fitted = (attachment.record.fit_band_mm != 0)
                .then(|| {
                    let hat = (*measured.get_or_insert_with(|| hat_line(avatar)))?;
                    let scale = fit_scale(Some(hat.circumference), attachment.record.fit_band_mm)?;
                    Some(fitted_seat(&hat, avatar, socket, joint, scale))
                })
                .flatten();
            fitted.unwrap_or_else(|| seated_default(socket, avatar, joint))
        } else {
            // An authored offset — the gizmo's or the numeric editor's — is
            // always taken verbatim, fit included: arming the gizmo on a
            // fitted prop commits the then-current scale into the offset,
            // so manual control keeps the size the wearer saw.
            transform_from_data(&attachment.record.offset)
        };
        out.push((joint, transform, attachment));
    }
    out
}

/// The uniform worn-subtree scale a fit declaration asks for (#1089): the
/// wearer's measured brow circumference as an equivalent-circle diameter,
/// over the item's authored band inner diameter. `None` — no measurement,
/// or no fit declared — is the authored-size fallback, deliberately quiet:
/// a creature head or an unmeasurable body wears the prop exactly as a
/// pre-fit client would.
pub(crate) fn fit_scale(circumference: Option<f32>, fit_band_mm: u32) -> Option<f32> {
    if fit_band_mm == 0 {
        return None;
    }
    let authored = fit_band_mm as f32 / 1000.0;
    let fitted = circumference? / std::f32::consts::PI / authored;
    fitted.is_finite().then_some(fitted)
}

/// The measured hat line of one built head: what a fitted headband seats
/// against. Heights are head-local metres (relative to the head joint), the
/// unit every [`symbios_avatar::face::Skull`] profile speaks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HatLine {
    /// The head joint the measurement is anchored to.
    pub(crate) head: usize,
    /// Horizontal perimeter of the vault at the hat line, metres.
    pub(crate) circumference: f32,
    /// Height of the hat line above the head joint, metres.
    pub(crate) height: f32,
}

/// The seat of a **fitted** headband (#1089): the prop origin lands on the
/// head's vertical axis at the measured hat line, with the fit scale
/// applied uniformly.
///
/// Deliberately NOT [`symbios_avatar::Socket::seat`]: the generic seat
/// pushes an anchor *out of* the body so a first-attach prop is visible —
/// measured on the crown it lands 8–9 cm **forward** of the head axis
/// (the horizontal-push rule with a straight-up anchor), which is right
/// for a pendant and wrong for a band that must encircle the head. A
/// fitted band's authoring convention is therefore: the band circles the
/// **origin**, in the X–Z plane, at `y = 0` — and this seat puts that
/// origin exactly where the measurement says the band belongs.
fn fitted_seat(
    hat: &HatLine,
    avatar: &symbios_avatar::Avatar,
    socket: symbios_avatar::Socket,
    joint: usize,
    scale: f32,
) -> Transform {
    let world = avatar.rig.joints[hat.head].position + Vec3::Y * hat.height;
    Transform {
        translation: world - avatar.rig.joints[joint].position,
        rotation: outward_yaw(socket),
        scale: Vec3::splat(scale),
    }
}

/// The wearer's hat line — where it sits and how far round it is — measured
/// from the built body through the engine's public measure surface.
///
/// The hat line is found the way a hatter finds it: the largest horizontal
/// perimeter of the cranial vault between the eye line and the dome's cap —
/// the line that runs just above the brow ridge and around the occiput.
/// Hunted rather than pinned to a landmark because the vault's widest line
/// moves with the head-breadth and face-length axes, and a band seated
/// anywhere narrower would slide down to it anyway.
///
/// Instruments, all public engine API: [`symbios_avatar::face::Skull`]
/// measures the built head (the same measured-not-planned argument as
/// `rig::Surface` — the mesh sits well inside the node radius);
/// [`symbios_avatar::Canon`] hands back the eye line so its constant is not
/// copied here (its docs record how copies drift); `Skull::surface_at`
/// answers "where is the surface at this height, in this direction", and
/// the perimeter is that polygon's length. The scan stops one band short of
/// the crown: the top band is closed to a pole and a perimeter there is
/// noise, not a hat line.
///
/// `None` for a body with no measurable head — hair never counts, the
/// engine measures the bare skull — and the caller falls back to authored
/// size. The eye-line ruler ignores its `EyeParams` argument for the
/// landmark read here (only pupil spacing reads it), so the default params
/// are not a guess smuggled in.
pub(crate) fn hat_line(avatar: &symbios_avatar::Avatar) -> Option<HatLine> {
    let skull = symbios_avatar::face::Skull::measure(&avatar.parts.body, &avatar.rig)?;
    let canon =
        symbios_avatar::Canon::measure(&avatar.rig, &skull, &symbios_avatar::EyeParams::default());
    let floor = canon.level;
    let (_, crown) = skull.throat_and_crown();
    let ceiling = crown - skull.crown_band();
    if ceiling <= floor {
        return None;
    }
    let mut best: Option<(f32, f32)> = None;
    for step in 0..=HAT_LINE_STEPS {
        let height = floor + (ceiling - floor) * step as f32 / HAT_LINE_STEPS as f32;
        let perimeter = perimeter_at(&skull, height);
        if best.is_none_or(|(widest, _)| perimeter > widest) {
            best = Some((perimeter, height));
        }
    }
    let (circumference, height) = best?;
    (circumference > f32::EPSILON && circumference.is_finite()).then_some(HatLine {
        head: skull.head,
        circumference,
        height,
    })
}

/// The head's horizontal perimeter at one height: the closed polygon of
/// [`symbios_avatar::face::Skull::surface_at`] samples around the full
/// turn. Engine-space (`glam`) arithmetic throughout — every sample shares
/// the height, so each chord is horizontal by construction.
fn perimeter_at(skull: &symbios_avatar::face::Skull, height: f32) -> f32 {
    let mut total = 0.0;
    let mut prev = skull.surface_at(height, 0.0);
    for step in 1..=HAT_LINE_AZIMUTHS {
        let azimuth = std::f32::consts::TAU * step as f32 / HAT_LINE_AZIMUTHS as f32;
        let next = skull.surface_at(height, azimuth);
        total += (next - prev).length();
        prev = next;
    }
    total
}

/// Give every joint entity the visibility components a worn prop's
/// inheritance chain needs. The engine spawns joints as bare transforms —
/// a rig is not a renderable — so parenting a `Visibility`-bearing prop
/// under one is Bevy's B0004 (an `InheritedVisibility` child below a
/// parent without it), warned at startup and undefined in behaviour.
/// `insert_if_new` keeps whatever a joint already carries; a few dozen
/// inserts per dress, only when an outfit actually changes.
pub(crate) fn ensure_joint_visibility(commands: &mut Commands, joints: &AvatarJoints) {
    for &joint in &joints.0 {
        commands.entity(joint).insert_if_new(Visibility::default());
    }
}

/// The engine-seated default placement: the socket's anchor pushed outside
/// the measured surface, expressed in the carrying joint's rest frame —
/// which is the frame the joint entity's children live in — and yawed so
/// the item's authored **+Z face points out of the body** ([`outward_yaw`]).
fn seated_default(
    socket: symbios_avatar::Socket,
    avatar: &symbios_avatar::Avatar,
    joint: usize,
) -> Transform {
    match socket.seat(&avatar.rig, &avatar.parts.surface, SEAT_MARGIN) {
        Some(anchor) => Transform {
            translation: anchor.position - avatar.rig.joints[joint].position,
            rotation: outward_yaw(socket),
            ..Transform::default()
        },
        None => Transform::default(),
    }
}

/// The default-seat yaw that turns an item's authored `+Z` face out of the
/// body — **the attachment authoring convention** (#1087): author a
/// wearable with `+Z` as the side meant to be seen, and a default seat
/// shows that side, whatever socket it lands on.
///
/// Per-socket rather than from the anchor, because a limb anchor's
/// `direction` is the bone axis (down a thigh, along a forearm), not an
/// outward normal. Engine body space has left `+X`, forward `+Z` (pinned
/// by the engine's own side tests), so side sockets get a quarter turn and
/// the rear sockets a half. Sockets with no unambiguous facing — the
/// crown, the grips, the feet — stay unrotated; only yaw is ever applied,
/// never pitch or roll, so nothing tips a hat. An *authored* offset (the
/// gizmo's output) is always taken verbatim instead of this.
fn outward_yaw(socket: symbios_avatar::Socket) -> Quat {
    use std::f32::consts::{FRAC_PI_2, PI};
    use symbios_avatar::Socket;
    match socket {
        Socket::LeftShoulder | Socket::LeftHip => Quat::from_rotation_y(FRAC_PI_2),
        Socket::RightShoulder | Socket::RightHip => Quat::from_rotation_y(-FRAC_PI_2),
        Socket::Back | Socket::Tail => Quat::from_rotation_y(PI),
        _ => Quat::IDENTITY,
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
pub(super) mod tests {
    use super::*;
    use crate::pds::avatar::wardrobe::{AttachmentRecord, engine_default_for_did};
    use crate::pds::types::Fp3;
    use crate::pds::{Generator, TransformData};

    /// The three answers `dressed_by` distinguishes (#1112). The middle
    /// one is the fix: a rigged record whose references have not resolved
    /// says *nothing*, and must not be read as "wearing nothing".
    ///
    /// It was read that way, and every live-preview broadcast — which
    /// carries rkeys only, because `resolved` is `#[serde(skip)]` — stripped
    /// the wearer's whole outfit on arrival and re-dressed it a wardrobe
    /// round-trip later, re-running each fitted prop's measurement.
    #[test]
    fn an_unresolved_rig_says_nothing_rather_than_nothing_worn() {
        let mut record = crate::pds::AvatarRecord::wearing("3jzfcijpj2z2a");
        assert_eq!(
            dressed_by(&record),
            None,
            "references not resolved yet: keep what is standing"
        );

        if let Some(rig) = record.body.rigged_mut() {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body: engine_default_for_did("did:plc:dressed-by"),
                attachments: Vec::new(),
            });
        }
        assert_eq!(
            dressed_by(&record).map(<[_]>::len),
            Some(0),
            "resolved and wearing nothing is a real, empty answer"
        );

        let generator = crate::pds::AvatarRecord::default_for_seed(7);
        assert_eq!(
            dressed_by(&generator).map(<[_]>::len),
            Some(0),
            "a generator body wears no rig attachments at all"
        );
    }

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

    /// A world with every resource the spawn path reaches, one local chassis
    /// wearing one resolved crown, and the built body installed — the stage
    /// both #1077 tests play on, and `hotswap`'s #1104 guard.
    pub(in crate::player) fn dressed_app() -> (bevy::app::App, Entity) {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = bevy::app::App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            bevy::asset::AssetPlugin::default(),
        ));
        app.init_asset::<bevy::prelude::Mesh>();
        app.init_asset::<bevy::prelude::StandardMaterial>();
        app.init_asset::<bevy::prelude::Image>();
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
        app.init_asset::<crate::water::WaterMaterial>();
        // Everything `AvatarSpawnDeps` fans out to.
        app.init_resource::<crate::world_builder::image_cache::BlobImageCache>();
        app.init_resource::<crate::world_builder::audio_resolver::BlobAudioCache>();
        app.init_resource::<crate::water::WaterSurfaces>();
        app.init_resource::<crate::world_builder::lsystem::LSystemMaterialCache>();
        app.init_resource::<crate::world_builder::lsystem::LSystemMeshCache>();
        app.init_resource::<crate::world_builder::ShapeMaterialCache>();
        app.init_resource::<crate::world_builder::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMaterialCache>();
        app.init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::spatial_audio::BakedAudioCache>();
        app.insert_resource(crate::world_builder::fresh_texture_cache());
        app.init_resource::<crate::world_builder::compile::CompiledWorld>();
        app.init_resource::<crate::world_builder::compile::CompileJob>();
        app.init_resource::<crate::diagnostics::MetricsRegistry>();
        app.init_resource::<crate::diagnostics::SessionLog>();
        app.init_resource::<bevy::prelude::Time>();

        // A local chassis wearing one crown, resolved.
        let record = {
            let mut record = crate::pds::AvatarRecord::wearing("3jzfcijpj2z2a");
            if let Some(rig) = record.body.rigged_mut() {
                rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                    body: engine_default_for_did("did:plc:attachment-test"),
                    attachments: vec![worn(AttachmentRecord::new(
                        Generator::default(),
                        symbios_avatar::Socket::Crown,
                    ))],
                });
            }
            record
        };
        app.insert_resource(crate::state::LiveAvatarRecord(record));
        let chassis = app
            .world_mut()
            .spawn((
                crate::state::LocalPlayer,
                bevy::prelude::Transform::default(),
                bevy::prelude::GlobalTransform::default(),
            ))
            .id();
        let avatar = built();
        let mut take = Some(avatar);
        app.world_mut()
            .run_system_once(
                move |mut commands: bevy::prelude::Commands,
                      mut meshes: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::prelude::Mesh>,
                >,
                      mut materials: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::prelude::StandardMaterial>,
                >,
                      mut images: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::prelude::Image>,
                >,
                      mut bindposes: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>,
                >| {
                    let Some(avatar) = take.take() else { return };
                    super::super::rigged::install_built_body(
                        &mut commands,
                        chassis,
                        0.9,
                        avatar,
                        &[],
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    );
                },
            )
            .expect("installs");

        // Dress.
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("dresses");
        let mut props = app
            .world_mut()
            .query_filtered::<Entity, With<AttachmentRoot>>();
        let prop = props.single(app.world()).expect("one worn prop");
        (app, prop)
    }

    /// #1135: a body already dressed as its record says latches, so the
    /// per-frame value comparison over the whole `ResolvedAttachment` list —
    /// every worn prop's `Generator` tree — stops running.
    ///
    /// Sequence: dress once, then stand still. Before the latch, every
    /// subsequent frame re-derived the same answer.
    #[test]
    fn a_dressed_body_latches_so_the_outfit_compare_stops_running() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, _) = dressed_app();
        // `dressed_app` dresses on a frame where nothing was latched; the
        // next pass is the first that can conclude "already dressed".
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("second pass");

        let mut latched = app
            .world_mut()
            .query_filtered::<Entity, (With<AttachmentsSteady>, With<RiggedRoot>)>();
        assert_eq!(
            latched.iter(app.world()).count(),
            1,
            "the dressed root did not latch — the outfit compare will run every frame"
        );
    }

    /// And the latch releases on a record edit, or a wardrobe change would
    /// never reach the body. This is the failure a naive latch would ship:
    /// cheap, and permanently wrong.
    #[test]
    fn editing_the_record_releases_the_latch_and_the_outfit_follows() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, prop) = dressed_app();
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("latching pass");

        // Take the crown off — a real record edit, which trips the resource's
        // change tick exactly as an editor click would.
        {
            let mut live = app
                .world_mut()
                .resource_mut::<crate::state::LiveAvatarRecord>();
            if let Some(rig) = live.0.body.rigged_mut()
                && let Some(resolved) = rig.resolved.as_mut()
            {
                resolved.attachments.clear();
            }
        }
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("undress");

        assert!(
            app.world().get_entity(prop).is_err(),
            "the crown is still worn — the latch swallowed a record edit"
        );
        let mut latched = app
            .world_mut()
            .query_filtered::<Entity, With<AttachmentsSteady>>();
        assert_eq!(
            latched.iter(app.world()).count(),
            0,
            "the latch survived the edit that released it"
        );
    }

    /// The parts editor's addressing contract (#1098): every node of the
    /// LOCAL player's worn prop carries an `AttachmentPrim` keyed by its
    /// record, with the item root at the empty path — so a tree row, a
    /// scene pick and a gizmo target all resolve to the same entity.
    #[test]
    fn a_local_props_parts_carry_attachment_prim_markers() {
        let (mut app, prop) = dressed_app();
        let mut parts = app
            .world_mut()
            .query::<(Entity, &crate::world_builder::AttachmentPrim, &ChildOf)>();
        let found: Vec<(Entity, crate::world_builder::AttachmentPrim, Entity)> = parts
            .iter(app.world())
            .map(|(e, marker, child_of)| (e, marker.clone(), child_of.parent()))
            .collect();
        assert!(
            !found.is_empty(),
            "the worn prop's nodes carry part markers"
        );
        assert!(
            found
                .iter()
                .all(|(_, marker, _)| marker.rkey == "3jzfcijpj2z2a"),
            "every marker names the prop's record: {found:?}"
        );
        let roots: Vec<_> = found
            .iter()
            .filter(|(_, marker, _)| marker.path.is_empty())
            .collect();
        assert_eq!(roots.len(), 1, "exactly one item root");
        assert_eq!(
            roots[0].2, prop,
            "the item root hangs directly off the prop root that carries LocalAttachment"
        );
    }

    /// The full dress-and-detach cycle through the REAL system (#1077): the
    /// record loses the prop and the sync must take every entity it spawned
    /// back out of the world.
    #[test]
    fn detaching_takes_the_prop_and_its_whole_tree_out_of_the_world() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, prop) = dressed_app();
        // Every entity the prop carries, gathered before the detach so the
        // assertion afterwards is about THOSE entities and not about a query
        // that could simply have lost its marker.
        let mut tree = vec![prop];
        let mut cursor = 0;
        while cursor < tree.len() {
            if let Some(children) = app.world().get::<bevy::prelude::Children>(tree[cursor]) {
                tree.extend(children.iter());
            }
            cursor += 1;
        }

        // Detach: the record loses the prop, exactly as the editor tab does it.
        {
            let mut live = app
                .world_mut()
                .resource_mut::<crate::state::LiveAvatarRecord>();
            let rig = live.0.body.rigged_mut().expect("rigged");
            rig.resolved.as_mut().expect("resolved").attachments.clear();
            rig.attachments.clear();
        }
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("undresses");

        let survivors: Vec<Entity> = tree
            .iter()
            .copied()
            .filter(|&entity| app.world().get_entity(entity).is_ok())
            .collect();
        assert!(
            survivors.is_empty(),
            "detaching left {} of the prop's {} entities alive in the world",
            survivors.len(),
            tree.len(),
        );
    }

    #[test]
    fn a_prop_armed_with_the_gizmo_does_not_survive_a_body_rebuild_as_a_phantom() {
        // **The in-app repro of #1077, as found.** Arming the in-world gizmo
        // (#1062) detaches a worn prop from its joint — parentless is the
        // gizmo's working state — and every gizmo commit writes the record,
        // which is what schedules #1059's settle rebuild. When that rebuild
        // lands, the old root and its joints despawn and every PARENTED prop
        // dies through the cascade; the armed one has no ChildOf and
        // survives, tracked by no ledger, while the fresh root dresses a
        // duplicate. Detaching then removes the duplicate and the phantom
        // keeps floating where the drag left it — "detaching items does not
        // despawn them properly".
        //
        // The plain-detach test above passes with the sweep deleted, which is
        // exactly why this one exists: the defect needs the gizmo AND the
        // rebuild in the same scene.
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, prop) = dressed_app();

        // Arm the gizmo: detach from the joint and float at a world pose,
        // exactly as `attach_or_release_attachment` does it.
        app.world_mut()
            .entity_mut(prop)
            .remove::<ChildOf>()
            .insert(transform_gizmo_bevy::GizmoTarget::default());

        // A rebuild lands: the old root is despawned and a fresh body is
        // installed, exactly as `land_rigged_builds` does it.
        let mut roots = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf), With<super::super::rigged::RiggedRoot>>();
        let (old_root, child_of) = roots.single(app.world()).expect("one root");
        let chassis = child_of.parent();
        let stale = vec![old_root];
        let avatar = built();
        let mut take = Some(avatar);
        app.world_mut()
            .run_system_once(
                move |mut commands: bevy::prelude::Commands,
                      mut meshes: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::prelude::Mesh>,
                >,
                      mut materials: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::prelude::StandardMaterial>,
                >,
                      mut images: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::prelude::Image>,
                >,
                      mut bindposes: bevy::prelude::ResMut<
                    bevy::prelude::Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>,
                >| {
                    let Some(avatar) = take.take() else { return };
                    super::super::rigged::install_built_body(
                        &mut commands,
                        chassis,
                        0.9,
                        avatar,
                        &stale,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    );
                },
            )
            .expect("rebuild lands");

        // The precondition IS the defect: the armed prop survived the cascade
        // that killed the root it belonged to. Without this assert, a future
        // where the gizmo stops detaching props would leave the sweep
        // untested rather than unnecessary.
        assert!(
            app.world().get_entity(prop).is_ok(),
            "the armed prop died with the root — the leak this test guards is gone and \
             so is its reason to exist"
        );

        // The sync re-dresses the fresh root; the armed prop is exempt from
        // the sweep while the gizmo holds it.
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("re-dresses");
        assert!(
            app.world().get_entity(prop).is_ok(),
            "the sweep took the prop out from under an armed gizmo"
        );

        // Release: the gizmo drops its markers (against a dead parent, the
        // release path can only drop them — there is nothing to reattach to).
        app.world_mut()
            .entity_mut(prop)
            .remove::<transform_gizmo_bevy::GizmoTarget>();
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("sweeps");

        assert!(
            app.world().get_entity(prop).is_err(),
            "the orphaned prop is still in the world after the gizmo released it — the \
             phantom the owner saw"
        );
        // And the sweep counted what it did (#1078): a swept orphan outside
        // an editing session is the signal a NEW leak path exists, so the
        // counter has to actually count for the invariant to mean anything.
        assert_eq!(
            app.world()
                .resource::<crate::diagnostics::MetricsRegistry>()
                .counter_value(crate::diagnostics::names::RUNTIME_ATTACHMENT_ORPHANS_SWEPT_COUNT),
            1,
            "one swept orphan must count exactly once"
        );
        // And exactly one prop remains: the fresh root's own dress.
        let mut props = app
            .world_mut()
            .query_filtered::<Entity, With<AttachmentRoot>>();
        assert_eq!(
            props.iter(app.world()).count(),
            1,
            "the fresh body should wear exactly its one recorded prop"
        );
    }

    /// #1104: an edit to one worn prop replaces that prop alone. A second
    /// prop is worn, the crown's offset is nudged, and after the sync the
    /// second prop is the same entity it was while the crown is a new one.
    /// Before the per-prop diff the whole outfit was despawned and
    /// respawned on any change.
    #[test]
    fn an_attachment_edit_respawns_only_the_edited_prop() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, crown) = dressed_app();

        // Wear a second prop.
        {
            let mut live = app
                .world_mut()
                .resource_mut::<crate::state::LiveAvatarRecord>();
            let rig = live.0.body.rigged_mut().expect("rigged");
            let resolved = rig.resolved.as_mut().expect("resolved");
            resolved.attachments.push(ResolvedAttachment {
                rkey: String::from("3jzfcijpj2z2b"),
                record: AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Back),
            });
        }
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("dresses the second prop");
        let mut roots = app
            .world_mut()
            .query_filtered::<&AttachmentsApplied, With<RiggedRoot>>();
        let worn_before = roots.single(app.world()).expect("one root").spawned();
        assert_eq!(worn_before.len(), 2);
        assert_eq!(
            worn_before[0], crown,
            "the crown kept its entity through a wear"
        );
        let back = worn_before[1];

        // Nudge the crown's offset — the gizmo commit's write.
        {
            let mut live = app
                .world_mut()
                .resource_mut::<crate::state::LiveAvatarRecord>();
            let rig = live.0.body.rigged_mut().expect("rigged");
            let resolved = rig.resolved.as_mut().expect("resolved");
            resolved.attachments[0].record.offset.translation = Fp3([0.0, 0.05, 0.0]);
        }
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("re-dresses the crown");
        let worn_after = roots.single(app.world()).expect("one root").spawned();
        assert_eq!(worn_after.len(), 2);
        assert_ne!(worn_after[0], crown, "the edited crown is a fresh prop");
        assert!(
            app.world().get_entity(crown).is_err(),
            "the old crown is gone"
        );
        assert_eq!(
            worn_after[1], back,
            "the untouched back piece kept its entity"
        );
        assert!(app.world().get_entity(back).is_ok());
        let mut props = app
            .world_mut()
            .query_filtered::<Entity, With<AttachmentRoot>>();
        assert_eq!(
            props.iter(app.world()).count(),
            2,
            "no phantom, no duplicate"
        );
    }

    /// **#1107, reproduced.** A part under the gizmo is detached from its
    /// prop; editing that part (a material change from the parts editor)
    /// replaces the prop, whose despawn cascade cannot reach the floating
    /// part — the old geometry stayed in the world beside the new. After
    /// the sync exactly the fresh prop's parts may exist, and the detached
    /// one must be gone.
    #[test]
    fn editing_a_gizmo_held_part_leaves_no_ghost() {
        use crate::world_builder::AttachmentPrim;
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, prop) = dressed_app();
        let mut parts = app.world_mut().query::<(Entity, &AttachmentPrim)>();
        let (root_part, _) = parts
            .iter(app.world())
            .find(|(_, marker)| marker.path.is_empty())
            .expect("the item root carries a marker");
        let before = parts.iter(app.world()).count();

        // Arm the gizmo on the part, exactly as `attach_or_release_prim`
        // does it: out of the hierarchy, remembering its parent.
        app.world_mut()
            .entity_mut(root_part)
            .remove::<ChildOf>()
            .insert((
                crate::editor_gizmo::GizmoDetachedPrim {
                    original_parent: prop,
                },
                transform_gizmo_bevy::GizmoTarget::default(),
            ));

        // The parts editor's edit: any change to the item tree.
        {
            let mut live = app
                .world_mut()
                .resource_mut::<crate::state::LiveAvatarRecord>();
            let rig = live.0.body.rigged_mut().expect("rigged");
            let resolved = rig.resolved.as_mut().expect("resolved");
            resolved.attachments[0].record.item.transform.translation = Fp3([0.0, 0.1, 0.0]);
        }
        app.world_mut()
            .run_system_once(sync_rigged_attachments)
            .expect("replaces the prop");

        assert!(
            app.world().get_entity(root_part).is_err(),
            "the gizmo-held part survived its prop's replacement — the ghost"
        );
        assert!(
            app.world().get_entity(prop).is_err(),
            "the old prop is gone"
        );
        assert_eq!(
            parts.iter(app.world()).count(),
            before,
            "exactly the fresh prop's parts exist — no ghost, no duplicate"
        );
        let mut parented = app
            .world_mut()
            .query_filtered::<&ChildOf, With<AttachmentPrim>>();
        assert!(
            parented.iter(app.world()).count() == before,
            "every remaining part hangs in the fresh prop's hierarchy"
        );
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

    /// The authoring convention: a default seat yaws the item's `+Z` face
    /// out of the body — `+X` on the left side, `-Z` at the back — and
    /// leaves facing-ambiguous sockets (crown, grips) unrotated. Caught
    /// in the wild by the first wearable (#1087): a hip satchel authored
    /// face-on-`+Z` wore with its flap toward the avatar's front.
    #[test]
    fn default_seats_yaw_the_authored_face_out_of_the_body() {
        let avatar = built();
        let face = |socket| {
            let joint = symbios_avatar::Socket::joint(socket, &avatar.rig).expect("resolves");
            seated_default(socket, &avatar, joint).rotation * Vec3::Z
        };
        assert!(
            (face(symbios_avatar::Socket::LeftHip) - Vec3::X).length() < 1e-5,
            "a left-hip seat faces the item +X (body left)"
        );
        assert!(
            (face(symbios_avatar::Socket::RightHip) - Vec3::NEG_X).length() < 1e-5,
            "a right-hip seat faces the item -X (body right)"
        );
        assert!(
            (face(symbios_avatar::Socket::Back) - Vec3::NEG_Z).length() < 1e-5,
            "a back seat faces the item away from the chest"
        );
        assert!(
            (face(symbios_avatar::Socket::Crown) - Vec3::Z).length() < 1e-5,
            "a crown seat never tips or spins the item"
        );
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
                source: None,
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
            source: None,
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

    /// The measurement-fit guard (#1089): across seeded bodies the computed
    /// fit scale exists, stays in a wearable range, and actually *varies* —
    /// a fit that answers the same number on every head is an authored
    /// constant wearing a measurement's name. And the scale is not just
    /// computed but applied: the worn transform out of [`placements`]
    /// carries it uniformly.
    #[test]
    fn the_fit_scale_tracks_head_sizes_across_seeds() {
        use crate::pds::avatar::wardrobe::engine_default_for_seed;

        // The circlet's own declaration, straight from the registry so this
        // guard moves when the authored band does.
        let fit = crate::catalogue::by_slug("circlet")
            .expect("the circlet is registered")
            .wear_fit()
            .expect("the fit hero declares a fit");
        let band_mm = fit.band_mm();

        let mut scales = Vec::new();
        for seed in [0u64, 1, 2, 5, 7] {
            let avatar = symbios_avatar::Avatar::build_with(
                &engine_default_for_seed(seed),
                &symbios_avatar::AvatarConfig {
                    atlas: 128,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|| panic!("seeded body {seed} builds"));
            let hat = hat_line(&avatar)
                .unwrap_or_else(|| panic!("seed {seed}: a humanoid head measures"));
            assert!(
                (0.3..1.2).contains(&hat.circumference),
                "seed {seed}: brow circumference {} is not a head's",
                hat.circumference
            );
            let scale = fit_scale(Some(hat.circumference), band_mm)
                .unwrap_or_else(|| panic!("seed {seed}: a measured head fits"));
            assert!(
                (0.5..2.0).contains(&scale),
                "seed {seed}: fit scale {scale} is outside the wearable range \
                 — either the measurement or the authored diameter is off"
            );
            scales.push((seed, scale));

            // And placements() applies it: a fitted identity-offset record
            // dresses at exactly that uniform scale, seated on the head's
            // own axis at the hat line — NOT at the generic crown seat,
            // which stands 8–9 cm forward of the head (the render finding
            // that produced `fitted_seat`) — while an unfitted one keeps
            // the authored size at the generic seat.
            let mut fitted =
                AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
            fitted.fit_band_mm = band_mm;
            let plain = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
            let outfit = [worn(fitted), worn(plain)];
            let placed = placements(&avatar, &outfit);
            assert_eq!(placed.len(), 2);
            assert!(
                (placed[0].1.scale - Vec3::splat(scale)).length() < 1e-5,
                "seed {seed}: worn scale {:?} is not the computed fit {scale}",
                placed[0].1.scale
            );
            let joint = placed[0].0;
            let expected = avatar.rig.joints[hat.head].position + Vec3::Y * hat.height
                - avatar.rig.joints[joint].position;
            assert!(
                (placed[0].1.translation - expected).length() < 1e-5,
                "seed {seed}: a fitted band seats at {:?}, expected the hat line {expected:?}",
                placed[0].1.translation
            );
            assert_eq!(
                placed[1].1.scale,
                Vec3::ONE,
                "seed {seed}: an unfitted prop must keep its authored size"
            );
            assert!(
                (placed[1].1.translation - placed[0].1.translation).length() > 0.02,
                "seed {seed}: the generic seat and the fitted seat should differ — \
                 if they agree, `fitted_seat` has stopped earning its existence"
            );
        }
        let (min, max) = scales.iter().fold((f32::MAX, f32::MIN), |(lo, hi), s| {
            (lo.min(s.1), hi.max(s.1))
        });
        assert!(
            max / min > 1.05,
            "the fit scale is flat across seeds ({scales:?}) — it is not measuring the head"
        );
    }

    /// The quiet fallbacks: no fit declared, and no measurement to fit to.
    /// Both wear at authored size, neither is an error — a pre-#1089 record
    /// and a creature head must dress exactly as they always did.
    #[test]
    fn the_fit_falls_back_to_authored_size() {
        assert_eq!(fit_scale(Some(0.56), 0), None, "no fit declared");
        assert_eq!(fit_scale(None, 178), None, "no measurable head");
        let snug = fit_scale(Some(std::f32::consts::PI * 0.178), 178)
            .expect("a measured head and a declared fit scale");
        assert!(
            (snug - 1.0).abs() < 1e-4,
            "a head whose equivalent diameter equals the authored band wears at 1.0, got {snug}"
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
