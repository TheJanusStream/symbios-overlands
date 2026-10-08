//! Scrap wall - a Post-apocalyptic prop. A barrier of mismatched corrugated
//! and plate metal welded to leaning posts. Scatter clutter fencing the
//! holdout.
//!
//! Overhauled under #972. The owner found the bald tyre lying almost flat
//! with its lowest point 17 cm off the ground (#1439, live in #1435); it now
//! stands on its tread, slumped back against the tallest panel. The same pass
//! found three parts held by less than they seemed, each within the 3 cm the
//! floating check calls touching: the middle post stood 4 cm proud of the
//! panel it was meant to be welded to, the top wire ran 2.4 cm over the
//! posts' tops, and the wheel rim hung 13 cm in front of its panel, caught
//! only on a post. The posts now stand in one line through every panel's
//! front face, the wire runs through them, and the rim and the sign are let
//! into their panels' faces. The sign is old enamel now (#972's texture
//! refresh), crazed and rusting through its chips.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CORRUGATED_RUST, PLANK_GREY, RUST_BROWN, SIGN_YELLOW, STEEL_GREY, TIRE_BLACK, enamel, plank,
    rusted, sheet, tarp,
};

pub struct ScrapWall;

impl CatalogueEntry for ScrapWall {
    fn slug(&self) -> &'static str {
        "scrap_wall"
    }
    fn name(&self) -> &'static str {
        "Scrap Wall"
    }
    fn description(&self) -> &'static str {
        "Barrier of mismatched corrugated and plate metal welded to leaning posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Depth of the tallest panel, the root, which stands centred on `z = 0`.
const ROOT_D: f32 = 0.12;
/// The steel plate panel's centre depth and thickness.
const STEEL_Z: f32 = 0.0;
const STEEL_D: f32 = 0.14;
/// The short rusted panel's centre depth and thickness.
const RUST_Z: f32 = -0.04;
const RUST_D: f32 = 0.12;
/// The posts' stock, height and line. The line is chosen so that every
/// panel's front face lies between a post's front and back faces: each post
/// is welded into the panel behind it, 2 to 6 cm deep.
const POST: f32 = 0.12;
const POST_H: f32 = 2.3;
const POST_Z: f32 = -0.10;
/// How deep a part fixed to a face is let into it.
const LET_IN: f32 = 0.01;
/// The tyre: its tube and ring radii, and how far it leans back from upright.
const TYRE_TUBE: f32 = 0.16;
const TYRE_RING: f32 = 0.4;
const TYRE_LEAN: f32 = 0.26;

fn build_tree() -> Generator {
    let mut prims = vec![
        // Tallest corrugated panel - the root.
        prim(
            solid(cuboid_tapered(
                [1.4, 2.4, ROOT_D],
                0.0,
                sheet(CORRUGATED_RUST),
            )),
            [-1.2, 1.2, 0.0],
            id_quat(),
        ),
    ];
    // Mismatched panels of varying height welded alongside, each leaning its
    // own way - the lurching, never-plumb line of a scavenged barrier.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 2.0, STEEL_D], 0.0, rusted(STEEL_GREY))),
        [0.2, 1.0, STEEL_Z],
        quat_z(0.07),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 1.6, RUST_D], 0.0, sheet(RUST_BROWN))),
        [1.4, 0.8, RUST_Z],
        quat_z(-0.05),
    ));
    // A low salvaged plank patch nailed over the seam at the back.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.0, 0.1], 0.0, plank(PLANK_GREY))),
        [-0.55, 0.5, 0.08],
        quat_z(0.04),
    ));

    // Leaning support posts, one in front of each panel.
    for (i, x) in [-1.8_f32, 0.0, 1.9].into_iter().enumerate() {
        let lean = if i % 2 == 0 { 0.09 } else { -0.07 };
        prims.push(prim(
            solid(cuboid_tapered(
                [POST, POST_H, POST],
                0.0,
                rusted(STEEL_GREY),
            )),
            [x, POST_H * 0.5, POST_Z],
            quat_z(lean),
        ));
    }
    // A taut top wire strung through the posts just under their tops,
    // suggesting barbed defence; its ends are inside the two outer posts.
    prims.push(prim(
        solid(cylinder_tapered(0.025, 3.74, 4, 0.0, rusted(STEEL_GREY))),
        [-0.02, POST_H - 0.1, POST_Z],
        quat_z(FRAC_PI_2),
    ));
    // A wheel rim wired flat to the steel panel clear of the middle post, its
    // face turned to the camera (-Z).
    let rim_tube = 0.05;
    prims.push(prim(
        solid(torus(rim_tube, 0.26, rusted(STEEL_GREY))),
        [0.5, 1.4, STEEL_Z - STEEL_D * 0.5 - rim_tube + LET_IN],
        quat_x(FRAC_PI_2),
    ));
    // A faded enamel warning sign nailed up crooked on the rust panel.
    let sign_d = 0.04;
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.5, sign_d], 0.0, enamel(SIGN_YELLOW))),
        [1.35, 1.25, RUST_Z - RUST_D * 0.5 - sign_d * 0.5 + LET_IN],
        quat_z(0.2),
    ));
    prims.push(tyre());

    assemble(prims)
}

/// The bald tyre slumped against the back of the tallest panel (#1439):
/// stood on its tread facing `+Z`, then leant back `TYRE_LEAN` so its top
/// rests on the panel. Both contacts are let in, since the drawn tread and
/// tube are polygons inside the round ones.
fn tyre() -> Generator {
    let (lean_sin, lean_cos) = TYRE_LEAN.sin_cos();
    prim(
        solid(torus(TYRE_TUBE, TYRE_RING, tarp(TIRE_BLACK))),
        [
            -1.5,
            TYRE_RING * lean_cos + TYRE_TUBE - LET_IN,
            ROOT_D * 0.5 + TYRE_RING * lean_sin + TYRE_TUBE - LET_IN,
        ],
        quat_x(FRAC_PI_2 - TYRE_LEAN),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{Placed, assert_sanitize_stable, placed};
    use crate::pds::GeneratorKind;
    use bevy::prelude::Vec3;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ScrapWall.build(""), "scrap_wall");
    }

    /// The overhaul's two checks (#972, #1575). Neither saw this item's
    /// faults: the tyre and the loose post, wire and rim all touched
    /// something within the floating check's 3 cm - hence the guards below.
    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = ScrapWall.build("");
        assert_no_z_fighting(&built, "scrap_wall");
        assert_nothing_floats(&built, "scrap_wall");
    }

    /// How deep `point` lies inside a box part: the distance to its nearest
    /// face, negative outside. Read through the part's built transform, so
    /// a leaning post or panel is measured as it leans, not by the larger
    /// axis-aligned box round it.
    fn depth_in(part: &Placed, point: Vec3) -> f32 {
        let GeneratorKind::Cuboid { size, .. } = &part.node.kind else {
            return f32::MIN;
        };
        let local = part
            .world
            .compute_affine()
            .inverse()
            .transform_point3(point);
        (Vec3::from_array(size.0) * 0.5 - local.abs()).min_element()
    }

    /// The box parts whose declared size passes `keep`.
    fn boxes<'a>(parts: &'a [Placed<'a>], keep: fn([f32; 3]) -> bool) -> Vec<&'a Placed<'a>> {
        parts
            .iter()
            .filter(|p| matches!(&p.node.kind, GeneratorKind::Cuboid { size, .. } if keep(size.0)))
            .collect()
    }

    /// The panels: boxes declared over a metre wide and a metre tall.
    fn panels<'a>(parts: &'a [Placed<'a>]) -> Vec<&'a Placed<'a>> {
        boxes(parts, |s| s[0] > 1.0 && s[1] > 1.0)
    }

    /// **The tyre slumps on its tread against a panel** (#1439, seen live
    /// by the owner in #1435). The floating check cannot see this fault:
    /// the shipped tyre lay almost flat with its lowest point 17 cm up, but
    /// it touched the wall, so its group reached the ground through the
    /// panels. Read from the drawn tyre - the one torus in tyre rubber: its
    /// lowest point is on the ground; its ring leans back from upright by
    /// more than a few degrees (it rests on something rather than balancing)
    /// and less than 30 (it stands on its tread); its top leans toward the
    /// wall; the point it leans out to lies on a panel's back face; and of
    /// every box its drawn vertices reach into, only that panel holds any of
    /// it, by no more than 1.5 cm - it rests there, let in a centimetre, and
    /// cuts into nothing else (where it was first stood it ran 1.4 cm into
    /// the plank patch).
    #[test]
    fn the_tyre_slumps_on_its_tread_against_a_panel() {
        let built = ScrapWall.build("");
        let parts = placed(&built);
        let tyres: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(&p.node.kind, GeneratorKind::Torus { common, .. }
                    if common.material.base_color.0 == TIRE_BLACK)
            })
            .collect();
        assert_eq!(tyres.len(), 1, "scrap_wall: one tyre");
        let tyre = tyres[0];
        let drawn = tyre.drawn.expect("the tyre is drawn");
        assert!(
            (-0.03..=0.005).contains(&drawn.min.y),
            "scrap_wall: the tyre's lowest point is at {}, not on the ground",
            drawn.min.y
        );
        let normal = tyre.toward([0.0, 1.0, 0.0]);
        let lean = normal.y.abs().asin();
        assert!(
            (0.05..=0.5).contains(&lean),
            "scrap_wall: the tyre's ring leans {lean} rad from upright - flat on the \
             ground, or balanced on its tread with nothing to lean on"
        );
        // The ring's own up, in its plane: where its top leans.
        let up = (Vec3::Y - normal * normal.y).normalize();
        assert!(
            up.z < -0.05,
            "scrap_wall: the tyre's top leans {up}, not back toward the wall"
        );
        let rest = panels(&parts).into_iter().find(|panel| {
            let face = panel.drawn.expect("a panel is drawn");
            (face.max.z - drawn.min.z).abs() < 0.02
                && face.min.x < drawn.max.x
                && face.max.x > drawn.min.x
                && face.min.y < drawn.max.y
                && face.max.y > drawn.min.y
        });
        let Some(rest) = rest else {
            panic!(
                "scrap_wall: the tyre leans out to z {} and no panel's back face is there",
                drawn.min.z
            )
        };
        let mesh = crate::world_builder::build_primitive_mesh(&tyre.node.kind).mesh;
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("the tyre's mesh has positions");
        };
        // How deep its drawn vertices reach into each box: the panel it
        // rests on may hold a little of it, nothing else any.
        for other in boxes(&parts, |_| true) {
            let deepest = vertices
                .iter()
                .map(|v| depth_in(other, tyre.world.transform_point(Vec3::from_array(*v))))
                .fold(f32::MIN, f32::max);
            let allowed = if other.path == rest.path {
                0.015
            } else {
                0.002
            };
            assert!(
                deepest <= allowed,
                "scrap_wall: the tyre runs {deepest} m into the box at children{:?}",
                other.path
            );
        }
    }

    /// **Every post is welded into a panel, and the top wire runs through
    /// every post** - two holds the floating check's 3 cm of touching let
    /// pass: the middle post stood 4 cm proud of the panel behind it, and
    /// the wire ran 2.4 cm over the posts' tops. Read through each part's
    /// built transform, not its axis-aligned box (a leaning post's is a
    /// third of a metre wide): four points up each post's axis, a
    /// centimetre inside its back face, all lie in one panel; both ends of
    /// the wire (the one drum declared over 3 m long) lie inside a post;
    /// and every post holds a point of the wire's axis.
    #[test]
    fn every_post_is_welded_into_a_panel_and_carries_the_wire() {
        let built = ScrapWall.build("");
        let parts = placed(&built);
        let posts = boxes(&parts, |s| s[1] > 2.0 && s[0] < 0.2);
        assert_eq!(posts.len(), 3, "scrap_wall: three posts");
        let panels = panels(&parts);
        for post in &posts {
            let GeneratorKind::Cuboid { size, .. } = &post.node.kind else {
                unreachable!("selected as a box")
            };
            let back = size.0[2] * 0.5 - 0.01;
            let welded = panels.iter().any(|panel| {
                [-0.6_f32, -0.3, 0.0, 0.3]
                    .iter()
                    .all(|&y| depth_in(panel, post.at([0.0, y, back])) > 0.0)
            });
            assert!(
                welded,
                "scrap_wall: the post at children{:?} is welded into no panel",
                post.path
            );
        }
        let wires: Vec<&Placed> = parts
            .iter()
            .filter(|p| {
                matches!(&p.node.kind, GeneratorKind::Cylinder { height, .. } if height.0 > 3.0)
            })
            .collect();
        assert_eq!(wires.len(), 1, "scrap_wall: one top wire");
        let wire = wires[0];
        let GeneratorKind::Cylinder { height, .. } = &wire.node.kind else {
            unreachable!("selected as a drum")
        };
        let half = height.0 * 0.5;
        for end in [-half, half] {
            let at = wire.at([0.0, end, 0.0]);
            assert!(
                posts.iter().any(|post| depth_in(post, at) > 0.0),
                "scrap_wall: the wire's end at {at} is in no post"
            );
        }
        for post in &posts {
            let held = (0..=400).any(|k| {
                let y = -half + height.0 * k as f32 / 400.0;
                depth_in(post, wire.at([0.0, y, 0.0])) > 0.0
            });
            assert!(
                held,
                "scrap_wall: the wire does not run through the post at children{:?}",
                post.path
            );
        }
    }

    /// **The wheel rim and the sign lie flat on their panels, let into
    /// them**: the rim hung 13 cm in front of its panel, caught only on a
    /// post, and the sign was turned edge-on into its panel, one side buried
    /// and the other standing 9 cm proud. Each - the torus that is not the
    /// tyre, and the one plate declared under 5 cm thick - faces along the
    /// wall's depth; points across its back, 3 mm inside its surface, all
    /// lie in one panel (let in, and on the panel, not off its edge); and
    /// its middle does not (no more than half of it buried).
    #[test]
    fn the_rim_and_the_sign_lie_flat_on_their_panels() {
        let built = ScrapWall.build("");
        let parts = placed(&built);
        let panels = panels(&parts);
        let mut seen = 0;
        for part in &parts {
            // (its facing, points across its back, its middle), all local.
            let (what, facing, back, middle): (&str, [f32; 3], Vec<[f32; 3]>, [f32; 3]) =
                match &part.node.kind {
                    GeneratorKind::Torus {
                        major_radius,
                        minor_radius,
                        common,
                        ..
                    } if common.material.base_color.0 != TIRE_BLACK => {
                        let (r, t) = (major_radius.0, minor_radius.0 - 0.003);
                        let ring = [(r, 0.0), (0.0, r), (-r, 0.0), (0.0, -r)];
                        (
                            "the wheel rim",
                            [0.0, 1.0, 0.0],
                            ring.iter().map(|&(x, z)| [x, t, z]).collect(),
                            [r, 0.0, 0.0],
                        )
                    }
                    GeneratorKind::Cuboid { size, .. } if size.0[2] < 0.05 => {
                        let [hx, hy, hz] = size.0.map(|v| v * 0.5);
                        let z = hz - 0.003;
                        (
                            "the sign",
                            [0.0, 0.0, 1.0],
                            [
                                (0.8, 0.8),
                                (-0.8, 0.8),
                                (0.8, -0.8),
                                (-0.8, -0.8),
                                (0.0, 0.0),
                            ]
                            .iter()
                            .map(|&(fx, fy)| [fx * hx, fy * hy, z])
                            .collect(),
                            [0.0, 0.0, 0.0],
                        )
                    }
                    _ => continue,
                };
            seen += 1;
            let facing = part.toward(facing);
            assert!(
                facing.z.abs() > 0.999,
                "scrap_wall: {what} faces {facing}, not square to the wall"
            );
            let on = panels.iter().find(|panel| {
                back.iter()
                    .all(|&point| depth_in(panel, part.at(point)) > 0.0)
            });
            let Some(panel) = on else {
                panic!("scrap_wall: {what} is not let into any one panel across its back")
            };
            assert!(
                depth_in(panel, part.at(middle)) <= 0.0,
                "scrap_wall: {what} is buried past its middle in its panel"
            );
        }
        assert_eq!(seen, 2, "scrap_wall: a wheel rim and a sign");
    }
}
