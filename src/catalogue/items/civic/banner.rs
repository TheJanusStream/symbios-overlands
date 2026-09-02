//! Banner — a tall pole flying a long hanging banner under a gilt finial. A
//! prosperity-Rich scatter prop: heraldic / civic display signals pride and
//! means in any setting.
//!
//! One fix under #972 (user-found in-world: "the pointed finial reads
//! upside-down"). The spear point was a [`cone`] given `quat_x(PI)` — a
//! half-turn that was presumably meant to stand it up, but a cone's apex is
//! already `+Y`, so the turn stood it on its point with the wide base in
//! the air. It is now at the identity, and its base is sunk into the orb
//! where the orb is still wider than the cone (#972 lesson 33b: an apex is
//! not a shelf, and neither is the top of a sphere).
//!
//! Corollary to #972 lessons 14 and 23, worth its own line: **check which
//! way the primitive already points before rotating it.** Every revolved
//! prim here has its axis on `+Y` and a cone's apex is its `+Y` end; the
//! only half-turn a cone ever needs is the one that makes a funnel. The
//! guard reads the built quaternion and asks where the apex landed.

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{CANVAS_RED, GOLD, WOOD, bronze, cloth, wood};

const POLE_H: f32 = 3.4;
const POLE_R: f32 = 0.08;
/// The gilt orb the point stands in, sunk onto the pole top.
const ORB_R: f32 = 0.09;
const ORB_Y: f32 = POLE_H + 0.04;
/// Spear point: base radius, height, and how far its base sits below the
/// orb's crown. At that depth the orb's own radius is
/// `sqrt(ORB_R² − (ORB_R − sink)²)`, which has to exceed `POINT_R`.
const POINT_R: f32 = 0.07;
const POINT_H: f32 = 0.3;
const POINT_SINK: f32 = 0.04;

pub struct Banner;

impl CatalogueEntry for Banner {
    fn slug(&self) -> &'static str {
        "banner"
    }
    fn name(&self) -> &'static str {
        "Banner"
    }
    fn description(&self) -> &'static str {
        "Tall pole flying a long banner under a gilt finial."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    use std::f32::consts::PI;
    let bz = -0.10; // banner hangs forward of the pole, toward the -Z front.

    let crossarm_y = POLE_H - 0.25;
    let banner_top = crossarm_y - 0.05;
    let field_drop = 1.66;
    let chief_h = 0.22;
    let field_y = banner_top - chief_h - field_drop * 0.5;
    let field_bottom = banner_top - chief_h - field_drop;
    // The point's base, sunk into the orb below its crown.
    let point_base = ORB_Y + ORB_R - POINT_SINK;

    let mut prims = vec![
        // Pole.
        prim(
            solid(cylinder_tapered(POLE_R, POLE_H, 10, 0.0, wood(WOOD))),
            [0.0, POLE_H * 0.5, 0.0],
            id_quat(),
        ),
        // Crossbar the gonfalon hangs from, bridging pole to banner.
        prim(
            solid(cuboid_tapered([1.0, 0.06, 0.06], 0.0, wood(WOOD))),
            [0.0, crossarm_y, bz * 0.5],
            id_quat(),
        ),
        // Chief band (the contrasting top stripe).
        prim(
            cuboid_tapered([0.95, chief_h, 0.04], 0.0, cloth(GOLD)),
            [0.0, banner_top - chief_h * 0.5, bz],
            id_quat(),
        ),
        // Main red field.
        prim(
            cuboid_tapered([0.95, field_drop, 0.04], 0.0, cloth(CANVAS_RED)),
            [0.0, field_y, bz],
            id_quat(),
        ),
        // Gilt fringe band near the foot of the field.
        prim(
            cuboid_tapered([0.95, 0.08, 0.05], 0.0, cloth(GOLD)),
            [0.0, field_bottom + 0.06, bz],
            id_quat(),
        ),
        // Gold emblem charge — a disc straddling both faces so it reads
        // front and back, not painted on one side.
        prim(
            solid(cylinder_tapered(0.26, 0.12, 12, 0.0, bronze(GOLD))),
            [0.0, field_y + 0.1, bz],
            quat_x(PI * 0.5),
        ),
        // Spear-point finial: an orb sunk onto the pole top, and the point
        // seated in the orb with its apex — the cone's own +Y — up.
        prim(sphere(ORB_R, 3, bronze(GOLD)), [0.0, ORB_Y, 0.0], id_quat()),
        prim(
            cone(POINT_R, POINT_H, 8, bronze(GOLD)),
            [0.0, point_base + POINT_H * 0.5, 0.0],
            id_quat(),
        ),
        // Decorative pole bands.
        prim(
            torus(0.02, 0.085, bronze(GOLD)),
            [0.0, crossarm_y - 0.4, 0.0],
            id_quat(),
        ),
        prim(torus(0.02, 0.085, bronze(GOLD)), [0.0, 0.5, 0.0], id_quat()),
    ];

    // Swallowtail tails with the central notch between them.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.34, 0.5, 0.04], 0.0, cloth(CANVAS_RED)),
            [sx * 0.28, field_bottom - 0.25, bz],
            id_quat(),
        ));
        // Tassel hanging from each tail.
        prims.push(prim(
            sphere(0.05, 3, bronze(GOLD)),
            [sx * 0.28, field_bottom - 0.52, bz],
            id_quat(),
        ));
    }

    super::assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
    };
    use crate::pds::GeneratorKind;

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Banner.build(""), "banner");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Banner.build(""), "banner");
    }

    /// **The point points up, and it is seated in the orb.** The apex is
    /// found by rotating the cone's own `+Y` half-height by its BUILT
    /// quaternion (#972 lesson 23), so a half-turn that flips it shows up as
    /// an apex below the base. Then the orb has to be wider than the cone at
    /// the height the cone's base sits (lesson 33b), and the orb has to be
    /// sunk onto the pole rather than resting on it. Against the shipped
    /// `quat_x(PI)` this fails with the apex 0.3 m below the base.
    #[test]
    fn the_finial_points_up_and_is_seated_in_the_orb() {
        let root = Banner.build("");
        let mut point: Option<([f32; 3], f32, f32, [f32; 4])> = None;
        let mut orb: Option<([f32; 3], f32)> = None;
        let mut pole_top = 0.0_f32;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cone { radius, height, .. } => {
                point = Some((at, radius.0, height.0, g.transform.rotation.0))
            }
            GeneratorKind::Sphere { radius, .. } if at[1] > POLE_H - 0.5 => {
                orb = Some((at, radius.0))
            }
            GeneratorKind::Cylinder { height, .. } if height.0 > 3.0 => {
                pole_top = at[1] + height.0 * 0.5
            }
            _ => {}
        });
        let (at, r, h, q) = point.expect("a spear point");
        let (orb_at, orb_r) = orb.expect("an orb under the point");
        let tip = rotate_by(q, [0.0, h * 0.5, 0.0]);
        let (apex, base) = (at[1] + tip[1], at[1] - tip[1]);
        assert!(
            apex > base,
            "banner: the finial's apex is at {apex} and its base at {base} — it is standing \
             on its point"
        );
        assert!(
            tip[0].abs() < 1e-4 && tip[2].abs() < 1e-4,
            "the point leans: its axis is {tip:?}"
        );
        // The orb's radius at the height of the cone's base.
        let dy = base - orb_at[1];
        assert!(
            dy.abs() < orb_r,
            "banner: the point's base at {base} is outside the orb (centre {}, r {orb_r})",
            orb_at[1]
        );
        let orb_r_there = (orb_r * orb_r - dy * dy).sqrt();
        assert!(
            orb_r_there >= r,
            "banner: at the point's base the orb is {orb_r_there} m across and the point is \
             {r} — balanced on the crown, not seated"
        );
        assert!(
            orb_at[1] - orb_r < pole_top,
            "banner: the orb rests on the pole top at {pole_top} instead of being sunk onto it"
        );
    }

    /// The gonfalon hangs forward of the pole toward the render front, and
    /// every cloth panel is thin in `Z` so its broad faces look down `-Z`.
    #[test]
    fn the_cloth_hangs_toward_the_front_with_its_broad_face_forward() {
        let mut panels = 0;
        walk(&Banner.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, common, .. } = &g.kind
                && matches!(
                    common.material.texture,
                    crate::pds::SovereignTextureConfig::Fabric(_)
                )
            {
                panels += 1;
                assert!(
                    at[2] < -0.05,
                    "banner: a cloth panel at {at:?} is not forward of the pole"
                );
                assert!(
                    size.0[2] < size.0[0] && size.0[2] < size.0[1],
                    "banner: a cloth panel {:?} is not thin in Z",
                    size.0
                );
            }
        });
        assert_eq!(panels, 5, "chief, field, fringe and two tails");
    }
}
