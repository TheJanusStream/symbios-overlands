//! Radio mast - a Post-apocalyptic secondary. A tall scrap-lattice mast braced
//! with salvaged steel, a dish hung off its face, an antenna whip on the plate
//! that caps it and a red warning light on the whip's tip. The lifeline of the
//! holdout; its light is emissive trim the ruin pass can darken.
//!
//! Overhauled under #972 for #1439, two defects the owner found live (#1435).
//! The whip started in the open middle of the lattice, held by nothing; now
//! the legs end in a cap plate and the whip stands on it. The dish was a solid
//! half-ball as wide as the mast, the front legs running through it and its
//! feed horn floating off its axis; now it is a shallow shell hung in front of
//! the lattice on a bracket from a front cross-bar, aimed along that bracket,
//! with the horn at its focus on a tripod from the rim. Both are the owner's
//! live fixes, built here, and the guards below read each relation off the
//! built tree.
//!
//! Built as a tree that stands the way the mast does (#972 lesson 3): the base
//! carries the lattice, the cap plate the antenna and the front cross-bar the
//! dish, so one drag in the editor moves a whole sub-assembly. Its sound is
//! the kit's desolate wind, heard steady: the bake holds its 0.18 Hz swell
//! still (#1385).

use std::f32::consts::{FRAC_PI_2, FRAC_PI_6};

use crate::catalogue::items::util::{
    aim_y, assemble, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, quat_x,
    quat_z, solid, sphere, strut, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, DISH_WHITE, RUST_BROWN, SIGNAL_RED, STEEL_GREY, concrete, enamel, fx, rusted,
};

pub struct RadioMast;

impl CatalogueEntry for RadioMast {
    fn slug(&self) -> &'static str {
        "radio_mast"
    }
    fn name(&self) -> &'static str {
        "Radio Mast"
    }
    fn description(&self) -> &'static str {
        "Tall scrap-lattice mast with a salvaged dish, an antenna whip and a red warning light."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Height of the concrete base the legs stand on.
const BASE_H: f32 = 0.5;
/// Length of the lattice legs, from the base to the cap plate.
const MAST_H: f32 = 12.0;
/// Half the span between leg centres.
const SPREAD: f32 = 0.9;
const LEG_R: f32 = 0.1;
/// The cap plate the antenna stands on: its side and thickness. It oversails
/// the legs' outer faces (`2 * (SPREAD + LEG_R)` across), and the legs end at
/// its mid-plane, buried in it.
const PLATE: f32 = 2.2;
const PLATE_T: f32 = 0.06;
/// The antenna whip: its height and foot radius. It tapers to half at the tip.
const WHIP_H: f32 = 3.0;
const WHIP_R: f32 = 0.05;
/// The dish: the top quarter of a sphere's profile (`DISH_CUT`, 45 degrees
/// round from the pole) of this radius, as a shell `1 - DISH_HOLLOW` of it
/// thick - 1.7 m across and 0.35 m deep, its focus half the radius in front
/// of its vertex.
const DISH_R: f32 = 1.2;
const DISH_CUT: [f32; 2] = [0.75, 1.0];
const DISH_HOLLOW: f32 = 0.95;
/// How far above the horizon the dish looks, out over the front (`-Z`).
const DISH_ELEVATION: f32 = 0.35;
/// Height of the front cross-bar the dish's bracket is clamped to.
const DISH_BAR_Y: f32 = BASE_H + 8.5;
/// The bracket's length from the bar to the dish's vertex: long enough that
/// the dish's back, which rises a little behind its vertex, clears the front
/// legs.
const BRACKET: f32 = 0.4;
/// The feed horn, a short can flaring to its mouth, which faces the dish.
const HORN_R: f32 = 0.07;
const HORN_L: f32 = 0.24;
/// How far round from the dish's axis the tripod's feet leave the shell -
/// inside its 45-degree rim, so each foot is buried in it.
const TRIPOD_FOOT: f32 = 0.733;

fn build_tree() -> Generator {
    let mast_top = BASE_H + MAST_H;

    let mut prims = vec![
        // Concrete base - the root.
        prim(
            solid(cuboid_tapered(
                [2.0, BASE_H, 2.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, BASE_H * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(footing(2.0, 2.0, [0.0, 0.0], 5.0));

    // Four lattice legs.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(LEG_R, MAST_H, 6, 0.0, rusted(STEEL_GREY))),
            [sx * SPREAD, BASE_H + MAST_H * 0.5, sz * SPREAD],
            id_quat(),
        ));
    }
    // Horizontal cross-braces ringing the lattice at four heights.
    let levels = [BASE_H, BASE_H + 3.0, BASE_H + 7.0, BASE_H + 10.5];
    for &h in &levels[1..] {
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.06, 0.06, 2.0 * SPREAD],
                    0.0,
                    rusted(STEEL_GREY),
                )),
                [sx * SPREAD, h, 0.0],
                id_quat(),
            ));
        }
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [2.0 * SPREAD, 0.06, 0.06],
                    0.0,
                    rusted(STEEL_GREY),
                )),
                [0.0, h, sz * SPREAD],
                id_quat(),
            ));
        }
    }
    // Zig-zag diagonal braces filling each bay - the scrap-lattice density a
    // bare four-post frame lacks. Direction alternates per bay for the truss.
    let span = 2.0 * SPREAD;
    for (b, w) in levels.windows(2).enumerate() {
        let (y0, y1) = (w[0], w[1]);
        let dy = y1 - y0;
        let len = (span * span + dy * dy).sqrt();
        let ang = dy.atan2(span) * if b % 2 == 0 { 1.0 } else { -1.0 };
        let ymid = (y0 + y1) * 0.5;
        // Front + back faces (bar along X, tilted about Z).
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([len, 0.05, 0.05], 0.0, rusted(STEEL_GREY))),
                [0.0, ymid, sz * SPREAD],
                quat_z(ang),
            ));
        }
        // Left + right faces (bar along Z, tilted about X).
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.05, 0.05, len], 0.0, rusted(STEEL_GREY))),
                [sx * SPREAD, ymid, 0.0],
                quat_x(-ang),
            ));
        }
    }

    prims.push(antenna(mast_top));
    prims.push(dish_mount());

    let mut root = assemble(prims);
    // Signature life: desolate wind through the lattice.
    root.audio = fx::desolate_wind();
    root
}

/// The cap plate on the leg tops and the antenna it carries (#1439): the
/// whip stands on the plate, its foot let into it, the cross-element rides
/// the whip and the warning light sits on its tip - nested in that order,
/// so the plate carries the lot.
fn antenna(mast_top: f32) -> Generator {
    let foot = mast_top + PLATE_T * 0.5 - 0.02;
    let tip = foot + WHIP_H;
    // Warning light - emissive.
    let light = prim(
        sphere(0.18, 3, glow(SIGNAL_RED, 3.0)),
        [0.0, tip + 0.1, 0.0],
        id_quat(),
    );
    let cross = prim(
        solid(cuboid_tapered([1.6, 0.06, 0.06], 0.0, rusted(RUST_BROWN))),
        [0.0, foot + 0.6, 0.0],
        id_quat(),
    );
    let whip = nest(
        prim(
            solid(cylinder_tapered(WHIP_R, WHIP_H, 6, 0.5, rusted(RUST_BROWN))),
            [0.0, foot + WHIP_H * 0.5, 0.0],
            id_quat(),
        ),
        vec![cross, light],
    );
    nest(
        prim(
            solid(cuboid_tapered(
                [PLATE, PLATE_T, PLATE],
                0.0,
                rusted(STEEL_GREY),
            )),
            [0.0, mast_top, 0.0],
            id_quat(),
        ),
        vec![whip],
    )
}

/// The salvaged dish (#1439) and what holds it, one decision (#972 lesson
/// 39, a head faces along its stalk): a front cross-bar between the front
/// legs, a bracket out from it along the dish's aim, the dish on the
/// bracket's end, and the feed horn at the focus on three struts from the
/// rim. The bar is the sub-root, square to the lattice; everything turned
/// hangs off it as a leaf (#972 lesson 22).
fn dish_mount() -> Generator {
    let (rise, run) = DISH_ELEVATION.sin_cos();
    let aim = [0.0, rise, -run];
    let along = |p: [f32; 3], t: f32| [p[0] + aim[0] * t, p[1] + aim[1] * t, p[2] + aim[2] * t];

    let bar_at = [0.0, DISH_BAR_Y, -SPREAD];
    let vertex = along(bar_at, BRACKET);
    let shell = DISH_R * (1.0 - DISH_HOLLOW);
    // A sphere cut to its top cap is concave toward the sphere's centre, down
    // its own -Y: turning +Y against the aim faces the aperture along it, and
    // puts the centre a radius out in front of the vertex.
    let centre = along(vertex, DISH_R);
    let focus = along(vertex, DISH_R * 0.5);

    let dish = prim(
        solid(with_cut(
            sphere(DISH_R, 4, enamel(DISH_WHITE)),
            [0.0, 1.0],
            DISH_CUT,
            DISH_HOLLOW,
        )),
        centre,
        aim_y([-aim[0], -aim[1], -aim[2]]),
    );
    // The bracket's dish end lands mid-shell at the vertex.
    let bracket = strut(
        bar_at,
        along(vertex, shell * 0.5),
        0.05,
        6,
        rusted(STEEL_GREY),
    );
    // Mouth at the focus, flaring toward the dish.
    let horn = prim(
        cylinder_tapered(HORN_R, HORN_L, 8, 0.4, rusted(RUST_BROWN)),
        along(focus, HORN_L * 0.5),
        aim_y(aim),
    );

    let mut parts = vec![bracket, dish, horn];
    // The tripod: one foot below, two above, each from mid-shell just inside
    // the rim to a point on the horn's axis inside the horn.
    let across = [0.0, run, rise]; // square to the aim, upward
    let mid_shell = DISH_R - shell * 0.5;
    let (out, back) = TRIPOD_FOOT.sin_cos();
    let into_horn = along(focus, HORN_L * 0.4);
    for theta in [-FRAC_PI_2, FRAC_PI_6, 5.0 * FRAC_PI_6] {
        let (s, c) = theta.sin_cos();
        let foot = [
            centre[0] + mid_shell * (-back * aim[0] + out * (c + s * across[0])),
            centre[1] + mid_shell * (-back * aim[1] + out * s * across[1]),
            centre[2] + mid_shell * (-back * aim[2] + out * s * across[2]),
        ];
        parts.push(strut(foot, into_horn, 0.018, 4, rusted(RUST_BROWN)));
    }

    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * SPREAD, 0.06, 0.06],
                0.0,
                rusted(STEEL_GREY),
            )),
            bar_at,
            id_quat(),
        ),
        parts,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{
        Placed, assert_no_tilted_parents, assert_sanitize_stable, placed,
    };
    use crate::pds::{GeneratorKind, PrimCommon};
    use bevy::prelude::{Vec2, Vec3};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RadioMast.build(""), "radio_mast");
    }

    /// The overhaul's two checks (#972, #1575): no faces drawn in one place,
    /// and nothing free of the body - which the whip, with its cross-element
    /// and light, and the feed horn all were (#1439).
    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = RadioMast.build("");
        assert_no_z_fighting(&built, "radio_mast");
        assert_nothing_floats(&built, "radio_mast");
    }

    /// The dish, its bracket, horn and struts are turned leaves under a
    /// square bar, and the whip stands square on the plate (lesson 22).
    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_parent() {
        assert_no_tilted_parents(&RadioMast.build(""), "radio_mast");
    }

    fn drawn(p: &Placed) -> crate::catalogue::items::measure::Bounds {
        p.drawn.expect("a primitive is drawn")
    }

    /// The four lattice legs: the drums drawn taller than 10 m.
    fn legs<'a>(parts: &'a [Placed<'a>]) -> Vec<&'a Placed<'a>> {
        let legs: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(p.node.kind, GeneratorKind::Cylinder { .. }) && drawn(p).size().y > 10.0
            })
            .collect();
        assert_eq!(legs.len(), 4, "radio_mast: four lattice legs");
        legs
    }

    /// **The antenna stands on the mast** (#1439; #972 lesson 33, a mast is
    /// a stack and chains from the ground to its top). Read from the drawn
    /// boxes: the four legs end in one cap, the whip - the one upright drum
    /// standing above the legs - has its foot in that cap, and the warning
    /// light overlaps the whip's tip on its axis. Against the shipped build
    /// the legs ended in the open air and the whip stood on nothing.
    #[test]
    fn the_antenna_stands_on_a_capped_mast() {
        let built = RadioMast.build("");
        let parts = placed(&built);
        let tops: Vec<Vec3> = legs(&parts)
            .iter()
            .map(|leg| {
                let b = drawn(leg);
                Vec3::new(b.center().x, b.max.y, b.center().z)
            })
            .collect();
        let caps: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(p.node.kind, GeneratorKind::Cuboid { .. })
                    && tops.iter().all(|t| drawn(p).contains(*t, 0.0))
            })
            .collect();
        assert_eq!(
            caps.len(),
            1,
            "radio_mast: the leg tops {tops:?} should all end in one cap"
        );
        let cap = drawn(caps[0]);

        let legs_end = tops.iter().map(|t| t.y).fold(f32::MIN, f32::max);
        let whips: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(p.node.kind, GeneratorKind::Cylinder { .. })
                    && p.toward([0.0, 1.0, 0.0]).y > 0.999
                    && drawn(p).min.y > legs_end - 0.1
                    && drawn(p).size().y > 1.0
            })
            .collect();
        assert_eq!(whips.len(), 1, "radio_mast: one whip above the legs");
        let whip = drawn(whips[0]);
        let foot = Vec3::new(whip.center().x, whip.min.y, whip.center().z);
        assert!(
            cap.contains(foot, 0.0),
            "radio_mast: the whip's foot {foot} is not let into the cap {cap:?}"
        );

        let lights: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(&p.node.kind, GeneratorKind::Sphere { common, .. }
                    if common.material.emission_strength.0 > 0.0)
            })
            .collect();
        assert_eq!(lights.len(), 1, "radio_mast: one warning light");
        let light = drawn(lights[0]);
        assert!(
            light.min.y < whip.max.y && light.max.y > whip.max.y,
            "radio_mast: the light spans {}..{}, not the whip's tip at {}",
            light.min.y,
            light.max.y,
            whip.max.y
        );
        let off_axis = Vec2::new(
            light.center().x - whip.center().x,
            light.center().z - whip.center().z,
        )
        .length();
        assert!(
            off_axis < 0.01,
            "radio_mast: the light is {off_axis} m off the whip's axis"
        );
    }

    /// **The dish faces its feed horn along its bracket, and hangs clear of
    /// the lattice** (#1439; #972 lesson 39, a head faces along its stalk).
    /// The dish is the one cut sphere: a cap of it, concave toward the
    /// sphere's centre, so its aperture faces the way its own `-Y` is
    /// turned, its vertex is its pole and its focus is halfway from the pole
    /// to the centre. Read off the built tree: the aperture looks out over
    /// the front; exactly two drums lie on its axis, both along it - the
    /// horn in front, mouth at the focus, and the bracket behind, one end in
    /// the shell at the vertex and the other in a bar; three thin struts
    /// reach from the rim to the horn's axis; and the dish is drawn wholly
    /// in front of the front legs. Against the shipped build: a half-ball
    /// the front legs ran through, and a horn lying along `Z` off its axis.
    #[test]
    fn the_dish_faces_its_horn_along_its_bracket() {
        let built = RadioMast.build("");
        let parts = placed(&built);
        let dishes: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(&p.node.kind, GeneratorKind::Sphere {
                    common: PrimCommon { torture, .. }, ..
                } if torture.profile_cut.0 != [0.0, 1.0])
            })
            .collect();
        assert_eq!(dishes.len(), 1, "radio_mast: one dish");
        let dish = dishes[0];
        let GeneratorKind::Sphere { radius, common, .. } = &dish.node.kind else {
            unreachable!("selected as a sphere")
        };
        let (r, hollow) = (radius.0, common.torture.hollow.0);
        // Everything below reads the cap round the sphere's +Y pole; a cut
        // kept elsewhere on the profile would be drawn somewhere else.
        let cut = common.torture.profile_cut.0;
        assert!(
            cut[1] > 0.999 && cut[0] > 0.5,
            "radio_mast: the dish keeps {cut:?} of its sphere's profile, not a cap round \
             its +Y pole"
        );
        let aim = dish.toward([0.0, -1.0, 0.0]);
        let centre = dish.at([0.0; 3]);
        let vertex = dish.at([0.0, r, 0.0]);
        let focus = dish.at([0.0, r * 0.5, 0.0]);
        assert!(
            aim.z < -0.8 && aim.y > 0.0,
            "radio_mast: the dish looks {aim}, not out over the front and up"
        );

        let ends = |p: &Placed, h: f32| [p.at([0.0, -h * 0.5, 0.0]), p.at([0.0, h * 0.5, 0.0])];
        let on_axis = |q: Vec3, base: Vec3| {
            let rel = q - base;
            (rel - aim * rel.dot(aim)).length()
        };
        let drums: Vec<(&Placed, f32)> = parts
            .iter()
            .filter_map(|p| match &p.node.kind {
                GeneratorKind::Cylinder { height, .. }
                    if on_axis(p.at([0.0; 3]), vertex) < 0.01 =>
                {
                    Some((p, height.0))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            drums.len(),
            2,
            "radio_mast: the dish's axis should carry its horn and its bracket"
        );
        for (drum, _) in &drums {
            assert!(
                drum.toward([0.0, 1.0, 0.0]).dot(aim).abs() > 0.999,
                "radio_mast: a drum on the dish's axis lies across it"
            );
        }
        let ahead = |p: &Placed| (p.at([0.0; 3]) - vertex).dot(aim);
        let &(horn, horn_h) = drums
            .iter()
            .find(|(p, _)| ahead(p) > 0.0)
            .expect("radio_mast: no horn in front of the dish");
        let &(bracket, bracket_h) = drums
            .iter()
            .find(|(p, _)| ahead(p) < 0.0)
            .expect("radio_mast: no bracket behind the dish");

        let mouth = ends(horn, horn_h)
            .into_iter()
            .min_by(|a, b| a.distance(vertex).total_cmp(&b.distance(vertex)))
            .expect("two ends");
        assert!(
            mouth.distance(focus) < 0.02,
            "radio_mast: the horn's mouth is {} m from the focus",
            mouth.distance(focus)
        );

        let [a, b] = ends(bracket, bracket_h);
        let (near, far) = if a.distance(vertex) < b.distance(vertex) {
            (a, b)
        } else {
            (b, a)
        };
        let into_shell = (near - vertex).dot(aim);
        assert!(
            (0.0..=r * (1.0 - hollow)).contains(&into_shell) && on_axis(near, vertex) < 0.01,
            "radio_mast: the bracket ends {into_shell} m along the axis from the vertex, \
             not in the shell"
        );
        assert!(
            parts.iter().any(|p| {
                matches!(p.node.kind, GeneratorKind::Cuboid { .. }) && drawn(p).contains(far, 0.0)
            }),
            "radio_mast: the bracket's foot {far} is clamped to no bar"
        );

        let horn_mid = horn.at([0.0; 3]);
        let in_horn =
            |q: Vec3| on_axis(q, horn_mid) < 0.01 && (q - horn_mid).dot(aim).abs() < horn_h * 0.5;
        let in_rim = |q: Vec3| {
            let d = q - centre;
            let round = d.normalize().dot(-aim).acos();
            (r * hollow..=r).contains(&d.length())
                && (0.6..std::f32::consts::FRAC_PI_4).contains(&round)
        };
        let struts = parts
            .iter()
            .filter(|p| match &p.node.kind {
                GeneratorKind::Cylinder { height, radius, .. } if radius.0 < 0.03 => {
                    let [a, b] = ends(p, height.0);
                    (in_horn(a) && in_rim(b)) || (in_horn(b) && in_rim(a))
                }
                _ => false,
            })
            .count();
        assert_eq!(
            struts, 3,
            "radio_mast: the horn should ride three struts from the dish's rim"
        );

        let legs_front = legs(&parts)
            .iter()
            .map(|leg| drawn(leg).min.z)
            .fold(f32::MAX, f32::min);
        let reach = drawn(dish).max.z;
        assert!(
            reach < legs_front - 0.05,
            "radio_mast: the dish reaches back to z {reach} and the front legs start at \
             {legs_front}"
        );
    }
}
