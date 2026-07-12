//! Shared glue for the universal default parts: the data-driven
//! [`FnPart`] table row + its [`BodyPart`] impl, and the small colour /
//! seeded-choice helpers every family file uses.

use crate::pds::generator::Generator;
use crate::seeded_defaults::ChassisFamily;

use super::super::{BodyPart, PartCtx, PartSlot};

/// Multiply a colour toward black by `f` (`0` = black, `1` = unchanged) —
/// the local "darker shade of the same hue" used for trousers / skirts /
/// bumpers so a second large surface stays tonally related to the primary.
pub(super) fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [c[0] * f, c[1] * f, c[2] * f]
}

// ---------------------------------------------------------------------------
// Value-contrast colour maths (#786/#787)
// ---------------------------------------------------------------------------
//
// Shared by the vehicle families (boat / skiff) to spread the palette triad
// across their surfaces and floor/separate the *values* so a dark or
// low-contrast seed still keeps readable part boundaries.

/// Perceptual-ish sRGB luma for value-contrast bookkeeping.
pub(super) fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Linear mix of `c` toward `target` by `t` (clamped to gamut).
pub(super) fn mix(c: [f32; 3], target: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (c[0] * (1.0 - t) + target[0] * t).clamp(0.0, 1.0),
        (c[1] * (1.0 - t) + target[1] * t).clamp(0.0, 1.0),
        (c[2] * (1.0 - t) + target[2] * t).clamp(0.0, 1.0),
    ]
}

/// Retint `c` to hit `target_l` luma by mixing toward white (to brighten) or
/// black (to darken) — keeps the hue, moves only the value.
pub(super) fn to_value(c: [f32; 3], target_l: f32) -> [f32; 3] {
    let l = luma(c);
    if (l - target_l).abs() < 1e-3 {
        return c;
    }
    if target_l > l {
        mix(
            c,
            [1.0, 1.0, 1.0],
            ((target_l - l) / (1.0 - l).max(1e-3)).clamp(0.0, 0.85),
        )
    } else {
        mix(
            c,
            [0.0, 0.0, 0.0],
            ((l - target_l) / l.max(1e-3)).clamp(0.0, 0.85),
        )
    }
}

/// Raise `c`'s value to at least `min_l` (a dark seed's body never collapses
/// to an unreadable near-black block).
pub(super) fn floor_value(c: [f32; 3], min_l: f32) -> [f32; 3] {
    if luma(c) < min_l {
        to_value(c, min_l)
    } else {
        c
    }
}

/// Push `c`'s value away from `ref_l` until they differ by at least
/// `min_delta` (staying on whichever side `c` already sits) — keeps two
/// adjacent vehicle surfaces (hull/deck, body/glass) from merging into one
/// mass on a low-contrast seed.
pub(super) fn ensure_delta(c: [f32; 3], ref_l: f32, min_delta: f32) -> [f32; 3] {
    let l = luma(c);
    if (l - ref_l).abs() >= min_delta {
        return c;
    }
    if l >= ref_l {
        to_value(c, (ref_l + min_delta).min(0.92))
    } else {
        to_value(c, (ref_l - min_delta).max(0.04))
    }
}

/// Deepen + saturate a colour toward its dominant channel — a running-light
/// glow wants to be a saturated jewel, not a pastel.
pub(super) fn saturate(c: [f32; 3]) -> [f32; 3] {
    let l = luma(c);
    [
        (c[0] + (c[0] - l) * 0.6).clamp(0.0, 1.0),
        (c[1] + (c[1] - l) * 0.6).clamp(0.0, 1.0),
        (c[2] + (c[2] - l) * 0.6).clamp(0.0, 1.0),
    ]
}

/// A data-driven [`BodyPart`] — metadata plus a build function pointer.
/// Universal default parts are plain enough to express as a table rather
/// than a struct apiece; the richer styled kits may use either.
pub(super) struct FnPart {
    pub(super) slug: &'static str,
    pub(super) slot: PartSlot,
    pub(super) chassis: &'static [ChassisFamily],
    pub(super) build: fn(&PartCtx) -> Generator,
}

impl BodyPart for FnPart {
    fn slug(&self) -> &'static str {
        self.slug
    }
    fn slot(&self) -> PartSlot {
        self.slot
    }
    fn chassis(&self) -> &'static [ChassisFamily] {
        self.chassis
    }
    fn build(&self, ctx: &PartCtx) -> Generator {
        (self.build)(ctx)
    }
    // styles() empty (universal) + ornateness/wear bands ANY by default.
}

// ---------------------------------------------------------------------------
// Humanoid
// ---------------------------------------------------------------------------
