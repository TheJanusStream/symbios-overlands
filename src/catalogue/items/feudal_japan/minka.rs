//! Minka — the Feudal-Japan *poor* landmark. A timber-framed farmhouse with
//! plaster-daub walls under a great steep thatched roof, hearth smoke
//! seeping through the ridge. The farmstead counterpart to the lacquered
//! [`pagoda`](super::pagoda): same theme, opposite end of the prosperity
//! axis (`Poor`), so a destitute room grows this instead of the temple.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    PAPER_CREAM, PLASTER_WHITE, STONE_GREY, THATCH_STRAW, TIMBER_BROWN, TIMBER_DARK, fx, paper,
    plaster, stone, thatch, timber,
};

pub struct Minka;

impl CatalogueEntry for Minka {
    fn slug(&self) -> &'static str {
        "minka"
    }
    fn name(&self) -> &'static str {
        "Minka Farmhouse"
    }
    fn description(&self) -> &'static str {
        "Timber-framed farmhouse with daub walls under a great steep thatch roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// How far the exposed corner posts stand proud of the daub face.
///
/// They were authored at `l * 0.5 - 0.2` with a half-width of `0.175`,
/// putting their outer faces 25 mm *inside* the wall planes at `l * 0.5` /
/// `w * 0.5` — so the "exposed" timber framing was entirely swallowed by
/// the plaster and only visible from indoors. This is the same authoring
/// slip as the pagoda's z-fighting columns (its `COLUMN_PROUD`) with the
/// sign flipped: there the post landed exactly on the wall plane and fought
/// it, here it landed behind the plane and vanished. Standing it out is what
/// *shinkabe*
/// framing does anyway — posts read, plaster infills between them.
const POST_PROUD: f32 = 0.07;

/// Half-width of a corner post (posts are `0.35` square).
const POST_HALF: f32 = 0.175;

fn build_tree() -> Generator {
    let l = 10.0_f32;
    let w = 7.0_f32;
    let foot_h = 0.4;
    let wall_h = 2.6;
    let wall_top = foot_h + wall_h;
    let roof_h = 3.6;

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, foot_h, w + 0.6],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Plaster-daub long walls and gable ends.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [l, wall_h, 0.3],
                0.0,
                plaster(PLASTER_WHITE),
            )),
            [0.0, foot_h + wall_h * 0.5, sz * (w * 0.5 - 0.15)],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.3, wall_h, w],
                0.0,
                plaster(PLASTER_WHITE),
            )),
            [sx * (l * 0.5 - 0.15), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Exposed timber corner posts, standing out of the daub on both faces
    // they meet — see [`POST_PROUD`].
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [POST_HALF * 2.0, wall_h, POST_HALF * 2.0],
                0.0,
                timber(TIMBER_DARK),
            )),
            [
                sx * (l * 0.5 + POST_PROUD - POST_HALF),
                foot_h + wall_h * 0.5,
                sz * (w * 0.5 + POST_PROUD - POST_HALF),
            ],
            id_quat(),
        ));
    }

    // Timber door framed on the −Z front (hero face), with a small shoji
    // window alongside it.
    let front_z = -(w * 0.5 - 0.15);
    prims.push(prim(
        solid(cuboid_tapered([1.5, 2.0, 0.2], 0.0, timber(TIMBER_BROWN))),
        [-l * 0.22, foot_h + 1.0, front_z - 0.08],
        id_quat(),
    ));
    // Door jambs.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.1, 0.24], 0.0, timber(TIMBER_DARK))),
            [-l * 0.22 + sx * 0.8, foot_h + 1.05, front_z - 0.08],
            id_quat(),
        ));
    }
    // Small shoji window beside the door: a dark timber surround framing the
    // cream paper pane crossed by kumiko muntins, all proud of the wall so the
    // opening reads against the pale plaster.
    let win_x = l * 0.2;
    let win_cy = foot_h + 1.5;
    let wall_front = front_z - 0.15;
    prims.push(prim(
        solid(cuboid_tapered([1.5, 1.15, 0.12], 0.0, timber(TIMBER_DARK))),
        [win_x, win_cy, wall_front - 0.06],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 0.85, 0.1], 0.0, paper(PAPER_CREAM))),
        [win_x, win_cy, wall_front - 0.14],
        id_quat(),
    ));
    for sx in [-1.0_f32, 0.0, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.07, 0.85, 0.05], 0.0, timber(TIMBER_DARK)),
            [win_x + sx * 0.38, win_cy, wall_front - 0.2],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([1.2, 0.07, 0.05], 0.0, timber(TIMBER_DARK)),
        [win_x, win_cy, wall_front - 0.2],
        id_quat(),
    ));

    // Great steep thatched roof, pinched to a long ridge along X (the minka
    // yosemune silhouette) rather than a square-topped frustum.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [l + 1.6, roof_h, w + 1.8],
            [0.12, 0.88],
            thatch(THATCH_STRAW),
        )),
        [0.0, wall_top + roof_h * 0.5, 0.0],
        id_quat(),
    ));
    // Ridge cap (munagi) bound along the crown.
    let ridge_x = 2.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [l - 1.0, 0.5, 0.8],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, wall_top + roof_h - 0.1, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: hearth smoke seeping from the ridge.
    root.children.push(fx::hearth_smoke(
        [ridge_x, wall_top + roof_h + 0.2, 0.0],
        0x70F0_CE11,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Minka.build(""), "minka");
    }
}
