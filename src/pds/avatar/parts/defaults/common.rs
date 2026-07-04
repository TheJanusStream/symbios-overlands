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
