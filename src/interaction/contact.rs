//! Contact data types — the wire format between the interaction producer
//! ([`super::classifier`]) and any consumer channel (shader feeders,
//! particle dispatchers, stain stampers).
//!
//! The types here intentionally carry only finished, world-space data —
//! consumers should not need to reach back into ECS to interpret a
//! sample. Adding a new surface kind is a matter of extending
//! [`SurfaceContact`] and [`SurfaceKind`] in lock-step; everything
//! downstream filters on those enums.
//!
//! [`AvatarContacts`] is rebuilt from scratch every frame — consumers
//! must read it in the same frame the producer writes it.

use bevy::prelude::*;

/// Lifecycle marker for a contact between an avatar and a surface.
///
/// The producer emits these by comparing the avatar's current surface
/// against its surface on the previous tick. A change of surface kind
/// (or, for water, surface index) emits one `Exit` sample for the old
/// surface immediately followed by one `Enter` sample for the new one;
/// staying on the same surface emits `Dwell` each frame.
///
/// Consumers typically use the phase to gate one-shot effects (a splash
/// on `Enter`, dust burst on `Exit`) versus continuous effects (wake
/// ripples on `Dwell`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContactPhase {
    /// First frame on this surface (or first frame after switching to a
    /// new surface variant / index).
    Enter,
    /// Continuous contact — same surface as last frame.
    Dwell,
    /// First frame after leaving this surface. The sample's `surface`
    /// field describes the surface that was just left; `world_pos` is
    /// the avatar's current position (which may now be off-surface).
    Exit,
}

/// Discriminant-only enum used by the producer's internal state to
/// answer "is this still the same kind of surface?" cheaply, and by
/// consumers that want to filter samples without matching the full
/// [`SurfaceContact`] payload.
///
/// Mirrors the variants of [`SurfaceContact`] — both must grow in
/// lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    Water,
    // Terrain: reserved for Phase 3.
}

/// World-space description of the surface the avatar is currently
/// engaged with. Returned wrapped in `Option<_>` by the producer's
/// internal probe step — `None` means "in the air, no contact".
///
/// Variants are intentionally non-exhaustive (a `Terrain` variant lands
/// in Phase 3 carrying `material_blend: [f32; 4]` and `normal: Vec3`).
/// Consumers should pattern-match the variants they handle and ignore
/// the rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceContact {
    /// Avatar is inside or touching a water plane.
    ///
    /// - `plane_idx` — index into
    ///   [`crate::water::WaterSurfaces::planes`]; lets a shader feeder
    ///   route impulses to the right material asset.
    /// - `depth` — positive value: how far below the surface the
    ///   query point sits, measured along the plane normal.
    /// - `flow_dir` — world XZ projection of the surface's downhill
    ///   tangent, zero on flat water.
    Water {
        plane_idx: usize,
        depth: f32,
        flow_dir: Vec2,
    },
}

impl SurfaceContact {
    /// Cheap discriminant for "same kind of surface as before?" checks.
    pub fn kind(&self) -> SurfaceKind {
        match self {
            SurfaceContact::Water { .. } => SurfaceKind::Water,
        }
    }
}

/// One avatar's contact reading for one frame. Multiple samples per
/// avatar per frame are possible (e.g. an `Exit` and an `Enter`
/// straddling a surface change).
#[derive(Debug, Clone, Copy)]
pub struct ContactSample {
    /// The avatar entity this sample describes.
    pub avatar: Entity,
    /// World-space chassis position at the time of sampling.
    pub world_pos: Vec3,
    /// World-space chassis linear velocity. Direct from `LinearVelocity`
    /// for local players; 1-frame finite-difference for remote peers
    /// (whose components do not currently carry the velocity).
    pub world_vel: Vec3,
    /// Effective radius the avatar occupies on this surface — see
    /// [`super::locomotion::LocomotionFootprint`]. Single source of
    /// truth for "scale my effect to the avatar's size".
    pub footprint_radius: f32,
    /// Which surface, with surface-kind-specific payload.
    pub surface: SurfaceContact,
    /// 0..1 normalised engagement. For water: `depth / total_height`
    /// of the avatar; saturates at 1 once the avatar is fully
    /// submerged. Lets consumers fade effect strength uniformly.
    pub intensity: f32,
    /// Lifecycle marker — see [`ContactPhase`].
    pub phase: ContactPhase,
}

/// Per-frame output of the contact classifier. Cleared and refilled
/// each tick.
#[derive(Resource, Default, Debug)]
pub struct AvatarContacts {
    pub samples: Vec<ContactSample>,
}

impl AvatarContacts {
    /// Iterator over samples that match a specific surface kind. Cheap
    /// helper for consumers that only care about, say, water.
    pub fn iter_kind(&self, kind: SurfaceKind) -> impl Iterator<Item = &ContactSample> {
        self.samples
            .iter()
            .filter(move |s| s.surface.kind() == kind)
    }
}
