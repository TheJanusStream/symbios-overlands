//! Fern - a low, dark fern rosette: eight fronds rising from one crown in
//! a shallow vase and arching over, dark green pinnae lit at their tips. It
//! is the Understory's `fern_rosette` as the agent Hypha saved it (#1708),
//! and it replaced the #910 L-system rosette (a crown that grew a frond a
//! year, coiled croziers relaxing as they aged) because the owner judged it
//! the better fern: Hypha's own word for the L-system's fern was "spiky
//! dark weeds".
//!
//! **Not an L-system, though the slug says so.** It is the fern recipe
//! ([`super::groundcover::fern`]): one eight-sided turned card wearing an
//! eight-frond `Needle` atlas, the shape of the catalogue's fern clump made
//! smaller and darker (seed 77, 16 pinna pairs, a stronger twist and a
//! deeper arch). The slug `lsys_fern` is kept because it is the seeded
//! jungles' and wetlands' fern stand (`TreeSpecies::Fern`) and the garden
//! bed's fern: renaming it would orphan every placed copy that names it.
//! What the L-system path did with it, and does now:
//!
//! - **Age.** A stand's `iterations_delta` and the garden bed's younger age
//!   step an L-system's iteration count; a turned card has none, so every
//!   fern grows at its one size, and the garden bed sizes its ferns by
//!   their scale alone.
//! - **Cost.** The stands' entity budget (#810) measures an L-system's
//!   expansion; this fern is counted by its node count, one part.
//! - **Skins.** It has no material variants, and no seeded pool or Berlin
//!   names one for it (`seeded_defaults::room::scatters` holds that).
//!
//! The entry is Hypha's saved generator to the ten-thousandth but for one
//! field: Hypha's root lift is 0.25 m, the fern clump's, which it kept when
//! it scaled the clump down to 0.72 of its size, so its crown hung 7 cm
//! over the ground. Here it is 0.18 m, the clump's lift at that scale, and
//! the crown stands on the ground.
//!
//! Measured with the render tool: 0.31 m tall and 0.88 x 0.88 m across, 80
//! triangles in 1 part (the L-system it replaced: 0.39 m tall and 1.2 m
//! across, 350 triangles in 2 parts).

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::Generator;

use super::groundcover::fern;

pub struct Fern;

impl CatalogueEntry for Fern {
    fn slug(&self) -> &'static str {
        "lsys_fern"
    }
    fn name(&self) -> &'static str {
        "Fern"
    }
    fn description(&self) -> &'static str {
        "Low, dark rosette of arching fern fronds - forest-floor shade flora."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        fern(
            77,
            16,
            [0.01, 0.055, 0.0072],
            [0.0397, 0.1329, 0.0134],
            0.4,
            [-0.04, -0.52, 0.0],
            0.18,
            [2.3879, 1.4967, 2.3879],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::GeneratorKind;

    #[test]
    fn build_round_trips_through_sanitize() {
        let built = Fern.build("");
        assert!(matches!(built.kind, GeneratorKind::Lathe { .. }));
        assert_sanitize_stable(&built, "lsys_fern");
    }

    /// Nothing draws a face where another does. The floating guard cannot
    /// fail for a fern of one part: its foot is held by the test below.
    #[test]
    fn the_fern_holds_together() {
        let built = Fern.build("");
        assert_no_z_fighting(&built, "lsys_fern");
        assert_nothing_floats(&built, "lsys_fern");
    }

    /// The fern is the clump's recipe made smaller: one part, its foot on
    /// the ground, standing lower and narrower than the clump.
    #[test]
    fn the_fern_is_a_smaller_clump() {
        let fern = Fern.build("");
        let clump = super::super::groundcover::FernClump.build("");
        assert!(fern.children.is_empty());
        let (f, c) = (fern.transform.scale.0, clump.transform.scale.0);
        for axis in 0..3 {
            let ratio = f[axis] / c[axis];
            assert!((ratio - 0.72).abs() < 0.001, "axis {axis}: {ratio}");
        }
        // The foot: the lowest station, lifted by the root.
        let GeneratorKind::Lathe { points, .. } = &fern.kind else {
            panic!("a lathe");
        };
        let foot = fern.transform.translation.0[1] + points[0].height.0 * f[1];
        assert!(foot.abs() < 0.001, "the foot stands at {foot} m");
    }
}
