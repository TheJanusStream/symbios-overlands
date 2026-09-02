//! Minivan — a Suburban prop. The family hauler: a rounded lower body with
//! a short sloping hood, a raked windscreen, a tall glazed greenhouse broken
//! by pillars under a flat roof with rails, and dark wheels with hub caps,
//! parked at the kerb. Nose toward `+X`; the broadside is what reads.
//!
//! Rebuilt from scratch under #972 after an in-world check ("looks very
//! clumsy and blocky and overall low quality"). The shipped van was three
//! tapered boxes — a body, a taller box on it and a `Window`-textured box
//! on that (#972 lesson 20 outright: the generator masks its panes away, so
//! the cabin was a frame with holes onto the box behind it, with window
//! frames across the roof) — plus slab pillars and slab bumpers. Now the
//! lower body and the nose are [`superellipsoid`]s (a box with a rolled
//! edge, the shape pressed steel actually takes), the hood is a [`wedge`]
//! turned to slope toward the nose, the windscreen and tailgate glass are
//! raked slabs whose ends are derived from the hood's rear edge and the
//! roof's front edge, the greenhouse is [`tinted_glass`] on solids, and the
//! wheels are tyres with proud hub caps standing on the ground.
//!
//! #972 lesson 38: **a car is a rolled edge, not a chamfer.** Boxes with a
//! taper read as "blocky" however many are stacked, because a taper is a
//! flat face at another angle. What reads as bodywork is a continuous
//! curve from side to top — one superellipsoid at a low exponent does it in
//! a single prim, and every detail then sits on a surface whose width at
//! any height is a function you can evaluate (the door handles here sit at
//! the body's own width at their own height, not at a round number).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, prim, quat_x,
    quat_y, quat_z, solid, superellipsoid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, enamel, tinted_glass};

/// Minivan body colour.
const VAN_BODY: [f32; 3] = [0.36, 0.40, 0.46];
/// Tyre black, hub-cap silver, trim dark.
const TIRE: [f32; 3] = [0.06, 0.06, 0.07];
const HUB: [f32; 3] = [0.62, 0.63, 0.66];
const TRIM: [f32; 3] = [0.16, 0.16, 0.18];

/// Overall: nose at `+X`, tail at `TAIL_X`, half-width `HALF_W`.
const NOSE_X: f32 = 2.3;
const TAIL_X: f32 = -2.3;
const HALF_W: f32 = 0.96;
/// Ground clearance under the body — the sill sits just under the axle
/// line, so the lower half of each tyre shows below it (at 0.2 the tyres
/// read as slivers under a slab).
const CLEAR: f32 = 0.32;
/// The belt line — the top of the lower body and the foot of the glass.
const BELT: f32 = 1.15;
/// Where the hood meets the body (the A-pillar foot) and its leading edge.
const HOOD_REAR_X: f32 = 1.35;
const HOOD_FRONT_Y: f32 = 0.9;
/// The roof: its underside is the head of the glass.
const GLASS_TOP: f32 = 1.66;
const ROOF_T: f32 = 0.08;
/// The windscreen's head — the roof's front edge.
const WS_TOP_X: f32 = 0.55;
/// The tailgate glass: near-vertical, its top a little forward of its foot.
const TAIL_GLASS_FOOT_X: f32 = -2.25;
const TAIL_GLASS_TOP_X: f32 = -2.15;
const GLASS_T: f32 = 0.04;
/// Wheels.
const WHEEL_X: f32 = 1.45;
const WHEEL_Z: f32 = 0.86;
const TYRE_R: f32 = 0.34;
const TYRE_W: f32 = 0.22;

pub struct Minivan;

impl CatalogueEntry for Minivan {
    fn slug(&self) -> &'static str {
        "minivan"
    }
    fn name(&self) -> &'static str {
        "Minivan"
    }
    fn description(&self) -> &'static str {
        "Tall boxy family minivan with a glazed greenhouse, parked at the kerb."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Superellipsoid exponent for pressed-steel bodywork: a box with a rolled
/// edge.
const ROLLED: f32 = 0.35;

/// The lower body's half-width at height `y`, from the superellipsoid's own
/// cross-section — where a side detail has to sit to touch the paint.
fn body_half_width_at(y: f32) -> f32 {
    let b = (BELT - CLEAR) * 0.5;
    let t = ((y - (BELT + CLEAR) * 0.5) / b).abs().min(1.0);
    let p = 2.0 / ROLLED;
    HALF_W * (1.0 - t.powf(p)).max(0.0).powf(1.0 / p)
}

/// A raked glass slab (and its pillars) from `foot` to `head` in the XY
/// plane, spanning `width` across the van: the rotation is the tilt that
/// takes the slab's own `+Y` from foot to head.
fn raked(
    foot: [f32; 2],
    head: [f32; 2],
    width: f32,
    t: f32,
    mat: crate::pds::SovereignMaterialSettings,
) -> Generator {
    let (dx, dy) = (head[0] - foot[0], head[1] - foot[1]);
    let len = (dx * dx + dy * dy).sqrt();
    // `quat_z(θ)` sends local +Y to (−sin θ, cos θ, 0).
    let theta = (-dx).atan2(dy);
    prim(
        solid(cuboid_tapered([t, len, width], 0.0, mat)),
        [(foot[0] + head[0]) * 0.5, (foot[1] + head[1]) * 0.5, 0.0],
        quat_z(theta),
    )
}

fn build_tree() -> Generator {
    let body = || enamel(VAN_BODY);
    let glass = || tinted_glass(GLASS_TINT);
    let ws_foot = [HOOD_REAR_X, BELT];
    let ws_head = [WS_TOP_X, GLASS_TOP];
    let tg_foot = [TAIL_GLASS_FOOT_X, BELT];
    let tg_head = [TAIL_GLASS_TOP_X, GLASS_TOP];
    let glass_w = HALF_W * 2.0 + 0.02;

    let mut prims = vec![
        // Lower body — the root: a rolled-edge box from the tail to the
        // A-pillar foot, sill to belt.
        prim(
            solid(superellipsoid(
                [(HOOD_REAR_X - TAIL_X) * 0.5, (BELT - CLEAR) * 0.5, HALF_W],
                ROLLED,
                ROLLED,
                body(),
            )),
            [(HOOD_REAR_X + TAIL_X) * 0.5, (BELT + CLEAR) * 0.5, 0.0],
            id_quat(),
        ),
        // Nose under the hood, a hair narrower so the panel line reads.
        prim(
            solid(superellipsoid(
                [
                    (NOSE_X - HOOD_REAR_X + 0.15) * 0.5,
                    (HOOD_FRONT_Y - CLEAR) * 0.5,
                    HALF_W - 0.01,
                ],
                ROLLED,
                ROLLED,
                body(),
            )),
            [
                (NOSE_X + HOOD_REAR_X - 0.15) * 0.5,
                (HOOD_FRONT_Y + CLEAR) * 0.5,
                0.0,
            ],
            id_quat(),
        ),
        // Hood: a wedge sloping down toward the nose. The wedge rises from
        // its local +Z to −Z; a quarter turn about Y puts +Z on world +X.
        prim(
            solid(wedge(
                [
                    HALF_W * 2.0 - 0.06,
                    BELT - (HOOD_FRONT_Y - 0.05),
                    NOSE_X - HOOD_REAR_X,
                ],
                body(),
            )),
            [
                (NOSE_X + HOOD_REAR_X) * 0.5,
                (BELT + HOOD_FRONT_Y - 0.05) * 0.5,
                0.0,
            ],
            quat_y(FRAC_PI_2),
        ),
        // Windscreen and tailgate glass, raked between derived edges.
        raked(ws_foot, ws_head, glass_w - 0.12, GLASS_T, glass()),
        raked(tg_foot, tg_head, glass_w - 0.12, GLASS_T, glass()),
        // Side glass band between them, proud of the body sides.
        prim(
            solid(cuboid_tapered(
                [WS_TOP_X - TAIL_GLASS_TOP_X, GLASS_TOP - BELT, glass_w],
                0.0,
                glass(),
            )),
            [
                (WS_TOP_X + TAIL_GLASS_TOP_X) * 0.5,
                (GLASS_TOP + BELT) * 0.5,
                0.0,
            ],
            id_quat(),
        ),
        // Roof over the glass, and its rails.
        prim(
            solid(cuboid_tapered(
                [WS_TOP_X - TAIL_GLASS_TOP_X + 0.1, ROOF_T, glass_w - 0.08],
                0.06,
                body(),
            )),
            [
                (WS_TOP_X + TAIL_GLASS_TOP_X) * 0.5,
                GLASS_TOP + ROOF_T * 0.5,
                0.0,
            ],
            id_quat(),
        ),
    ];
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([2.2, 0.05, 0.05], 0.0, enamel(TRIM)),
            [
                (WS_TOP_X + TAIL_GLASS_TOP_X) * 0.5,
                GLASS_TOP + ROOF_T + 0.025,
                sz * 0.72,
            ],
            id_quat(),
        ));
    }
    // Pillars: A and D ride the raked glass; B and C stand between the doors.
    for sz in [-1.0_f32, 1.0] {
        for (foot, head) in [(ws_foot, ws_head), (tg_foot, tg_head)] {
            let mut p = raked(foot, head, 0.08, 0.07, body());
            p.transform.translation.0[2] = sz * (HALF_W - 0.05);
            prims.push(p);
        }
    }
    // A centimetre past the glass at both ends, so the pillars' ends never
    // share the belt plane or the roof plane with the glass band.
    for px in [-0.45_f32, -1.4] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.09, GLASS_TOP - BELT + 0.02, glass_w + 0.02],
                0.0,
                body(),
            )),
            [px, (GLASS_TOP + BELT) * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Bumpers, chamfered, proud of nose and tail.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.24, 0.32, HALF_W * 2.0 - 0.04],
                [0.3, 0.15],
                enamel(TRIM),
            )),
            [sx * (NOSE_X + 0.02), 0.42, 0.0],
            id_quat(),
        ));
    }
    // Grille between the headlights, all proud of the nose.
    prims.push(prim(
        cuboid_tapered([0.03, 0.16, 0.9], 0.0, enamel(TRIM)),
        [NOSE_X + 0.015, 0.68, 0.0],
        id_quat(),
    ));
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.05, 0.14, 0.42], 0.0, glow([1.0, 0.95, 0.8], 2.2)),
            [NOSE_X + 0.015, 0.72, sz * 0.66],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.05, 0.42, 0.12], 0.0, glow([0.95, 0.12, 0.1], 2.0)),
            [TAIL_X - 0.015, 0.98, sz * 0.8],
            id_quat(),
        ));
    }

    // Mirrors on stalks at the A pillars; handles and a rub strip on the
    // body's own width at their height.
    for sz in [-1.0_f32, 1.0] {
        let w = body_half_width_at(1.0);
        prims.push(prim(
            cuboid_tapered([0.05, 0.03, 0.08], 0.0, enamel(TRIM)),
            [0.95, 1.25, sz * (HALF_W + 0.03)],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.12, 0.09, 0.14], 0.0, body()),
            [0.95, 1.25, sz * (HALF_W + 0.13)],
            id_quat(),
        ));
        for hx in [0.1_f32, -1.1] {
            prims.push(prim(
                cuboid_tapered([0.14, 0.03, 0.02], 0.0, enamel(TRIM)),
                [hx, 1.0, sz * (w + 0.008)],
                id_quat(),
            ));
        }
        let w = body_half_width_at(0.62);
        prims.push(prim(
            cuboid_tapered([3.2, 0.04, 0.02], 0.0, enamel(TRIM)),
            [-0.5, 0.62, sz * (w + 0.005)],
            id_quat(),
        ));
    }

    // Four wheels: tyre standing on the ground, hub cap proud of it.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(TYRE_R, TYRE_W, 16, 0.0, enamel(TIRE))),
            [sx * WHEEL_X, TYRE_R, sz * WHEEL_Z],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cylinder_tapered(0.2, TYRE_W + 0.02, 12, 0.0, enamel(HUB)),
            [sx * WHEEL_X, TYRE_R, sz * WHEEL_Z],
            quat_x(FRAC_PI_2),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, rotate_by,
    };
    use crate::pds::{GeneratorKind, PrimCommon};

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// A tilted slab's two ends along its own `Y`, low end first.
    fn ends(g: &Generator, at: [f32; 3], half: f32) -> ([f32; 3], [f32; 3]) {
        let tip = rotate_by(g.transform.rotation.0, [0.0, half, 0.0]);
        let a = [at[0] - tip[0], at[1] - tip[1], at[2] - tip[2]];
        let b = [at[0] + tip[0], at[1] + tip[1], at[2] + tip[2]];
        if a[1] <= b[1] { (a, b) } else { (b, a) }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Minivan.build(""), "minivan");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Minivan.build(""), "minivan");
    }

    #[test]
    fn no_two_faces_tie_for_depth() {
        assert_no_coplanar_faces(&Minivan.build(""), "minivan");
    }

    /// #972 lesson 20. The shipped greenhouse was a `Window` box.
    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&Minivan.build(""), "minivan");
    }

    /// **The windscreen runs from the hood's rear edge to the roof's front
    /// edge**, read from the built slab's ends; and the tailgate glass from
    /// the belt to the roof's rear edge, leaning forward.
    #[test]
    fn the_raked_glass_lands_on_the_belt_and_under_the_roof() {
        let root = Minivan.build("");
        let mut raked: Vec<([f32; 3], [f32; 3])> = Vec::new();
        let mut roof: Option<([f32; 3], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid {
                size,
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
            else {
                return;
            };
            let tilted = g.transform.rotation.0[2].abs() > 1e-4;
            if tilted
                && size.0[2] > 1.0
                && material.texture == crate::pds::SovereignTextureConfig::None
            {
                raked.push(ends(g, at, size.0[1] * 0.5));
            }
            if !tilted && size.0[0] > 2.5 && (size.0[1] - ROOF_T).abs() < 1e-4 {
                roof = Some((at, size.0));
            }
        });
        assert_eq!(raked.len(), 2, "a windscreen and a tailgate glass");
        let (roof_at, roof_s) = roof.expect("a roof");
        let roof_front = roof_at[0] + roof_s[0] * 0.5;
        let roof_back = roof_at[0] - roof_s[0] * 0.5;
        let roof_under = roof_at[1] - roof_s[1] * 0.5;
        for (foot, head) in raked {
            assert!(
                (foot[1] - BELT).abs() < 0.02,
                "glass foot at {foot:?} is off the belt"
            );
            assert!(
                (head[1] - roof_under).abs() < 0.02,
                "glass head at {head:?} misses the roof"
            );
            assert!(
                head[0] < roof_front && head[0] > roof_back,
                "glass head at {head:?} is outside the roof"
            );
            if foot[0] > 0.0 {
                assert!(
                    (foot[0] - HOOD_REAR_X).abs() < 0.02,
                    "the windscreen foot is off the hood's rear edge"
                );
                assert!(head[0] < foot[0] - 0.5, "the windscreen is not raked back");
            } else {
                assert!(head[0] > foot[0], "the tailgate glass leans the wrong way");
            }
        }
    }

    /// **The tyres stand on the ground and the body clears it.** Against the
    /// shipped build the tyres sit 20 mm under the ground plane.
    #[test]
    fn the_tyres_stand_on_the_ground_and_the_body_clears_it() {
        let root = Minivan.build("");
        let mut tyres = 0;
        let mut body_bottom = f32::MAX;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cylinder { radius, .. } if radius.0 > 0.3 => {
                let axis = rotate_by(g.transform.rotation.0, [0.0, 1.0, 0.0]);
                assert!(axis[2].abs() > 0.999, "a tyre's axle runs {axis:?}");
                tyres += 1;
                assert!(
                    (at[1] - radius.0).abs() < 0.005,
                    "minivan: a tyre at {at:?} of radius {} has its bottom at {}, not on the ground",
                    radius.0,
                    at[1] - radius.0
                );
            }
            GeneratorKind::Superellipsoid { half_extents, .. } => {
                body_bottom = body_bottom.min(at[1] - half_extents.0[1]);
            }
            GeneratorKind::Cuboid { size, .. }
                if g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0] && size.0[0] > 3.0 =>
            {
                body_bottom = body_bottom.min(at[1] - size.0[1] * 0.5);
            }
            _ => {}
        });
        assert_eq!(tyres, 4);
        assert!(
            body_bottom >= 0.15,
            "the body sits {body_bottom} off the ground"
        );
    }

    /// Every lamp is proud of the bodywork it is set in: the nose lamps'
    /// faces beyond the nose, the tail lamps' beyond the tail.
    #[test]
    fn every_lamp_stands_proud_of_the_bodywork() {
        let root = Minivan.build("");
        let (mut nose, mut tail) = (f32::MIN, f32::MAX);
        let mut lamps: Vec<([f32; 3], [f32; 3])> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Superellipsoid { half_extents, .. } => {
                nose = nose.max(at[0] + half_extents.0[0]);
                tail = tail.min(at[0] - half_extents.0[0]);
            }
            GeneratorKind::Cuboid { size, common, .. }
                if common.material.emission_strength.0 > 1.0 =>
            {
                lamps.push((at, size.0));
            }
            _ => {}
        });
        assert_eq!(lamps.len(), 4);
        for (at, s) in lamps {
            if at[0] > 0.0 {
                assert!(
                    at[0] + s[0] * 0.5 > nose,
                    "a nose lamp at {at:?} is buried in the nose ({nose})"
                );
            } else {
                assert!(
                    at[0] - s[0] * 0.5 < tail,
                    "a tail lamp at {at:?} is buried in the tail ({tail})"
                );
            }
        }
    }

    /// Side details sit on the body's own width at their height: their
    /// inner faces are within a centimetre of the superellipsoid's surface
    /// there (lesson 11 for a curved host).
    #[test]
    fn side_details_touch_the_paint() {
        let root = Minivan.build("");
        let mut checked = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, .. } = &g.kind
                && at[2].abs() > 0.5
                && size.0[2] < 0.03
                && at[1] < BELT
            {
                checked += 1;
                let inner = at[2].abs() - size.0[2] * 0.5;
                let skin = body_half_width_at(at[1]);
                assert!(
                    (inner - skin).abs() < 0.01,
                    "minivan: a side detail at {at:?} has its inner face at {inner} and the \
                     body's skin is at {skin} there"
                );
            }
        });
        assert_eq!(checked, 6, "two handles and a rub strip a side");
    }
}
