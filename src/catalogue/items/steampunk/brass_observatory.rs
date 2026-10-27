//! Brass observatory - a Steampunk secondary that leads a settlement from
//! its crest (#1796). A sooty brick drum trimmed with iron bands and a pair
//! of brass cogs over its door, under a riveted copper dome ribbed in brass,
//! its shutter slit open on the -Z side and a giant brass telescope jutting
//! out of it at the sky. Beside it a spiral stair climbs to an iron platform
//! carrying a clockwork orrery round a lamp-lit sun.
//!
//! Primitive-built on a footing; the drum, the platform and the stair are
//! each a subtree rooted where it stands, so the ruin pass leans each whole.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::catalogue::items::space_outpost::dome_ribs;
use crate::catalogue::items::util::{
    aim_y, attach, cuboid_tapered, cylinder_tapered, footing, glow, helix, id_quat, nest, prim,
    quat_mul, quat_x, quat_y, solid, sphere, strut, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, GAUGE_AMBER, IRON_DARK, LAMP_GAS, WOOD_BROWN, brass, brick,
    cog, copper, fx, iron, plank,
};

/// The spacing circle the settlement keeps round the observatory: the
/// terrace's half diagonal.
const CLEARANCE: f32 = 12.0;
/// The brick terrace it stands on, and the height of its top.
const TERRACE: f32 = 17.0;
const TERRACE_TOP: f32 = 0.6;
/// The drum: its centre on the terrace, its radius and its top.
const DRUM: [f32; 2] = [-2.0, 1.0];
const DRUM_R: f32 = 5.5;
const DRUM_TOP: f32 = 8.6;
/// The dome's radius.
const DOME_R: f32 = 5.4;
/// The orrery platform: its centre, its radius and its deck's top.
const DECK: [f32; 2] = [5.6, -1.0];
const DECK_R: f32 = 2.4;
const DECK_TOP: f32 = 4.2;
/// The steam vent pipe up the drum's back: where it stands and its top.
const VENT_Z: f32 = DRUM[1] + DRUM_R + 0.12;
const VENT_TOP: f32 = 10.2;

pub struct BrassObservatory;

impl CatalogueEntry for BrassObservatory {
    fn slug(&self) -> &'static str {
        "brass_observatory"
    }
    fn name(&self) -> &'static str {
        "Brass Observatory"
    }
    fn description(&self) -> &'static str {
        "Copper-domed brick observatory with a giant brass telescope, cogs and a clockwork orrery."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    /// Built high, under the open sky, as its kind is.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Crest
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut root = footing(TERRACE, TERRACE, [0.0, 0.0], CLEARANCE);
    attach(
        &mut root,
        prim(
            solid(cuboid_tapered(
                [TERRACE, TERRACE_TOP, TERRACE],
                0.0,
                brick([0.36, 0.31, 0.28]),
            )),
            [0.0, TERRACE_TOP * 0.5, 0.0],
            id_quat(),
        ),
    );
    attach(&mut root, drum());
    attach(&mut root, platform());
    attach(&mut root, stair());
    // Signature life: the drive that turns the dome, chugging, and a wisp of
    // steam from its vent.
    root.audio = fx::engine_chug();
    attach(
        &mut root,
        fx::steam_vent([DRUM[0], VENT_TOP + 0.1, VENT_Z], 0x0B5E_57A2),
    );
    root
}

/// The drum and everything it carries: iron bands, a brass cornice, the
/// door under its arch, lit portholes, the cogs over the door, and the dome.
fn drum() -> Generator {
    let [x, z] = DRUM;
    let h = DRUM_TOP - TERRACE_TOP + 0.02;
    let body = prim(
        solid(cylinder_tapered(DRUM_R, h, 28, 0.0, brick(BRICK_SOOT))),
        [x, TERRACE_TOP - 0.02 + h * 0.5, z],
        id_quat(),
    );
    let mut parts = Vec::new();
    for (k, y) in [1.3_f32, 5.2].into_iter().enumerate() {
        parts.push(prim(
            torus(0.12, DRUM_R + 0.02 + k as f32 * 0.005, iron(IRON_DARK)),
            [x, y, z],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(torus(0.24, DRUM_R + 0.06, brass(BRASS))),
        [x, DRUM_TOP - 0.1, z],
        id_quat(),
    ));
    // The door on the -Z face, under a brass arch on brass jambs.
    let front = z - DRUM_R;
    let door_h = 2.7_f32;
    parts.push(prim(
        solid(cuboid_tapered([1.7, door_h, 0.3], 0.0, plank(WOOD_BROWN))),
        [x, TERRACE_TOP + door_h * 0.5, front - 0.05],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.22, door_h + 0.1, 0.22], 0.0, brass(BRASS)),
            [
                x + sx * 0.93,
                TERRACE_TOP + (door_h + 0.1) * 0.5 - 0.02,
                front - 0.08,
            ],
            id_quat(),
        ));
    }
    parts.push(prim(
        with_cut(torus(0.12, 0.96, brass(BRASS)), [0.0, 0.5], [0.0, 1.0], 0.0),
        [x, TERRACE_TOP + door_h, front - 0.12],
        quat_x(-FRAC_PI_2),
    ));
    // Lit portholes round the sides, each in a brass rim.
    for a in [-0.55_f32, 0.55, PI - 0.6, PI + 0.6] {
        // Angles about the drum from its -Z front.
        let dir = [a.sin(), 0.0, -a.cos()];
        let at = |r: f32| [x + dir[0] * r, 4.0, z + dir[2] * r];
        parts.push(prim(
            cylinder_tapered(
                0.5,
                0.1,
                14,
                0.0,
                crate::catalogue::items::street::lit_from_dusk(glow(GAUGE_AMBER, 1.8)),
            ),
            at(DRUM_R + 0.01),
            aim_y(dir),
        ));
        parts.push(prim(
            torus(0.1, 0.55, brass(BRASS)),
            at(DRUM_R + 0.05),
            aim_y(dir),
        ));
    }
    // Two brass cogs over the door, the small one meshing at the big one's rim.
    let stand = quat_x(-FRAC_PI_2);
    parts.push(slim_teeth(cog(
        [x, 6.4, front - 0.02],
        stand,
        1.4,
        0.28,
        12,
        brass(BRASS),
        iron(IRON_DARK),
    )));
    let sx = 2.25_f32;
    let small_z = z - (DRUM_R * DRUM_R - sx * sx).sqrt();
    parts.push(slim_teeth(cog(
        [x + sx, 7.45, small_z - 0.06],
        stand,
        0.75,
        0.24,
        8,
        copper(COPPER_ORANGE),
        brass(BRASS),
    )));
    // The vent pipe up the back, above the cornice.
    parts.push(prim(
        solid(cylinder_tapered(
            0.2,
            VENT_TOP - 6.0,
            10,
            0.0,
            copper(COPPER_ORANGE),
        )),
        [x, (VENT_TOP + 6.0) * 0.5, VENT_Z],
        id_quat(),
    ));
    parts.push(dome());
    nest(body, parts)
}

/// A [`cog`] with its teeth a touch thinner than its blank, so their faces
/// stand inside the blank's rather than in its planes.
fn slim_teeth(mut gear: Generator) -> Generator {
    let blank = match &gear.kind {
        crate::pds::GeneratorKind::Cylinder { height, .. } => height.0,
        _ => return gear,
    };
    for tooth in &mut gear.children {
        if let crate::pds::GeneratorKind::Cuboid { size, .. } = &mut tooth.kind
            && (size.0[1] - blank).abs() < 1e-6
        {
            size.0[1] = blank * 0.9;
        }
    }
    gear
}

/// The copper dome: brass ribs, the shutter slit lit warm from within on
/// the -Z side between two brass rails, a finial at the crown, and the
/// telescope out of the slit.
fn dome() -> Generator {
    let [x, z] = DRUM;
    let c = [x, DRUM_TOP, z];
    let shell = prim(
        solid(with_cut(
            sphere(DOME_R - 0.06, 6, copper(COPPER_ORANGE)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        c,
        id_quat(),
    );
    let mut parts = dome_ribs(c, DOME_R, 5, brass(BRASS));
    // The slit: a narrow slice of the dome over the -Z meridian, from its
    // foot to the crown.
    parts.push(prim(
        with_cut(
            sphere(DOME_R + 0.04, 6, slit_light()),
            [0.725, 0.775],
            [0.5, 1.0],
            0.0,
        ),
        c,
        id_quat(),
    ));
    // Its rails: quarter arcs from the foot on -Z to the crown, either side.
    for sx in [-1.0_f32, 1.0] {
        let off = 0.95_f32;
        let r = (DOME_R * DOME_R - off * off).sqrt();
        parts.push(prim(
            with_cut(torus(0.1, r, brass(BRASS)), [0.0, 0.25], [0.0, 1.0], 0.0),
            [x + sx * off, DRUM_TOP, z],
            quat_mul(quat_y(FRAC_PI_2), quat_x(-FRAC_PI_2)),
        ));
    }
    // The finial: a brass spike on a ball.
    let crown = DRUM_TOP + DOME_R;
    parts.push(prim(
        solid(sphere(0.45, 5, brass(BRASS))),
        [x, crown + 0.2, z],
        id_quat(),
    ));
    parts.push(prim(
        cylinder_tapered(0.12, 2.6, 8, 0.9, brass(BRASS)),
        [x, crown + 1.8, z],
        id_quat(),
    ));
    parts.push(telescope());
    nest(shell, parts)
}

/// The open slit: dark by day, glowing amber from the lamp inside after
/// dusk.
fn slit_light() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.07, 0.06, 0.05]),
        emission_color: Fp3([1.0, 0.6, 0.24]),
        emission_strength: Fp(1.3),
        roughness: Fp(0.9),
        dusk_to_dawn: true,
        ..Default::default()
    }
}

/// The great telescope: a brass tube on the dome's pivot, raised 40 degrees
/// toward -Z, iron-banded, with a wide hood and a dark lens at its mouth.
fn telescope() -> Generator {
    let [x, z] = DRUM;
    let el = 40.0_f32.to_radians();
    let d = [0.0, el.sin(), -el.cos()];
    let pivot = [x, DRUM_TOP + 1.8, z];
    let at = |t: f32| {
        [
            pivot[0] + d[0] * t,
            pivot[1] + d[1] * t,
            pivot[2] + d[2] * t,
        ]
    };
    // The trunnion block on the pivot, inside the dome, carries the rest.
    let mount = prim(
        solid(cuboid_tapered([1.6, 1.0, 1.6], 0.0, iron(IRON_DARK))),
        pivot,
        id_quat(),
    );
    let mut parts = vec![strut(at(-2.0), at(8.6), 0.8, 18, brass(BRASS))];
    for t in [3.6_f32, 6.6] {
        parts.push(prim(torus(0.12, 0.82, iron(IRON_DARK)), at(t), aim_y(d)));
    }
    parts.push(prim(
        // Its bore grips the main tube's mouth.
        tube(1.05, 0.78, 1.4, 18, brass(BRASS)),
        at(8.9),
        aim_y(d),
    ));
    parts.push(prim(
        // Dark glass, unlit.
        cylinder_tapered(0.86, 0.06, 18, 0.0, glow([0.10, 0.16, 0.22], 0.0)),
        at(8.9),
        aim_y(d),
    ));
    // A finder scope riding its back.
    let up = [0.0, el.cos(), el.sin()];
    let side = |t: f32| {
        let p = at(t);
        [p[0], p[1] + up[1] * 1.05, p[2] + up[2] * 1.05]
    };
    parts.push(strut(side(4.2), side(7.2), 0.2, 10, brass(BRASS)));
    parts.push(strut(side(5.0), at(5.0), 0.07, 6, iron(IRON_DARK)));
    nest(mount, parts)
}

/// The orrery platform: an iron deck on two posts and a bracket to the drum,
/// railed, carrying a clockwork orrery - three planets on arms round a
/// lamp-lit brass sun, the outermost ringed.
fn platform() -> Generator {
    let [x, z] = DECK;
    // The posts stop short of the deck's top, inside it.
    let post_h = DECK_TOP - TERRACE_TOP - 0.06;
    let first = prim(
        solid(cylinder_tapered(0.16, post_h, 10, 0.0, iron(IRON_DARK))),
        [x + 1.4, TERRACE_TOP + post_h * 0.5, z - 1.4],
        id_quat(),
    );
    let deck = prim(
        solid(cylinder_tapered(DECK_R, 0.2, 24, 0.0, iron(IRON_DARK))),
        [x, DECK_TOP - 0.1, z],
        id_quat(),
    );
    // The orrery.
    let base = DECK_TOP;
    let ped_h = 1.3_f32;
    let sun = [x, base + ped_h + 0.4, z];
    let mut orrery = vec![
        prim(
            solid(cylinder_tapered(0.32, ped_h, 12, 0.35, brass(BRASS))),
            [x, base + ped_h * 0.5, z],
            id_quat(),
        ),
        prim(sphere(0.45, 6, glow(LAMP_GAS, 2.6)), sun, id_quat()),
    ];
    let planets = [
        (0.9_f32, 0.2_f32, 0.30_f32, copper(COPPER_ORANGE)),
        (2.6, 0.27, -0.15, iron([0.30, 0.34, 0.42])),
        (4.4, 0.24, 0.10, brass(BRASS)),
    ];
    for (k, (a, pr, dy, mat)) in planets.into_iter().enumerate() {
        let arm = [0.9 + k as f32 * 0.5, 0.0];
        let p = [
            sun[0] + a.cos() * arm[0],
            sun[1] + dy,
            sun[2] + a.sin() * arm[0],
        ];
        orrery.push(strut(sun, p, 0.04, 6, brass(BRASS)));
        orrery.push(prim(solid(sphere(pr, 5, mat)), p, id_quat()));
        if k == 2 {
            orrery.push(prim(
                tube(pr + 0.24, pr - 0.04, 0.03, 20, brass(BRASS)),
                p,
                quat_x(0.45),
            ));
        }
    }
    let mut deck_parts = orrery;
    // The railing round the deck's rim.
    deck_parts.push(prim(
        torus(0.05, DECK_R - 0.1, iron(IRON_DARK)),
        [x, DECK_TOP + 1.0, z],
        id_quat(),
    ));
    for k in 0..5 {
        let a = k as f32 / 5.0 * TAU + 0.9;
        deck_parts.push(prim(
            cuboid_tapered([0.07, 1.0, 0.07], 0.0, iron(IRON_DARK)),
            [
                x + a.cos() * (DECK_R - 0.1),
                DECK_TOP + 0.5,
                z + a.sin() * (DECK_R - 0.1),
            ],
            id_quat(),
        ));
    }
    let deck = nest(deck, deck_parts);
    let parts = vec![
        deck,
        prim(
            solid(cylinder_tapered(0.16, post_h, 10, 0.0, iron(IRON_DARK))),
            [x + 1.4, TERRACE_TOP + post_h * 0.5, z + 1.4],
            id_quat(),
        ),
        // A brace from the drum wall up under the deck.
        strut(
            [
                DRUM[0] + (DRUM_R * DRUM_R - (z - DRUM[1]).powi(2)).sqrt() - 0.1,
                DECK_TOP - 1.8,
                z,
            ],
            [x - 0.6, DECK_TOP - 0.15, z],
            0.12,
            8,
            iron(IRON_DARK),
        ),
    ];
    nest(first, parts)
}

/// The spiral stair to the platform: ten treads wound once round an iron
/// newel, an iron stringer coiling under their outer ends, the top tread
/// landing on the deck's -Z rim.
fn stair() -> Generator {
    const TREADS: usize = 10;
    let (x, z) = (DECK[0], DECK[1] - DECK_R - 0.95);
    let rise = (DECK_TOP - TERRACE_TOP) / TREADS as f32;
    let newel_h = DECK_TOP - TERRACE_TOP + 1.1;
    let newel = prim(
        solid(cylinder_tapered(0.15, newel_h, 10, 0.0, iron(IRON_DARK))),
        [x, TERRACE_TOP + newel_h * 0.5 - 0.02, z],
        id_quat(),
    );
    let mut parts = Vec::new();
    for k in 0..TREADS {
        // The top tread points at the deck (+Z); each below turns back by a
        // tenth of a turn, the way the stringer's coil runs.
        let a = TAU * (k + 1) as f32 / TREADS as f32 + FRAC_PI_2;
        let y = TERRACE_TOP + rise * (k + 1) as f32 - 0.06;
        parts.push(prim(
            solid(cuboid_tapered(
                [1.0, 0.08, 0.34],
                0.0,
                iron([0.30, 0.27, 0.24]),
            )),
            [x + a.cos() * 0.55, y, z + a.sin() * 0.55],
            quat_y(-a),
        ));
    }
    let pitch = DECK_TOP - TERRACE_TOP;
    parts.push(prim(
        helix(1.0, 0.05, pitch, 1.0, 40, brass(BRASS)),
        [x, TERRACE_TOP + pitch * 0.5, z],
        quat_y(-FRAC_PI_2),
    ));
    nest(newel, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BrassObservatory.build(""), "brass_observatory");
    }

    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = BrassObservatory.build("");
        assert_no_z_fighting(&built, "brass_observatory");
        assert_nothing_floats(&built, "brass_observatory");
    }

    /// The orrery's sun keeps the observatory lit.
    #[test]
    fn keeps_its_lamp() {
        assert!(has_emissive(&BrassObservatory.build("")));
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&BrassObservatory.build(""))
            .expect("serialises");
        assert!(bytes < 70_000, "{bytes} B");
    }

    /// The dome and its telescope stand tall: the finial tops 16 m.
    #[test]
    fn stands_tall() {
        let top = crate::catalogue::items::measure::solids(&BrassObservatory.build(""))
            .iter()
            .map(|s| s.bounds.max.y)
            .fold(0.0_f32, f32::max);
        assert!(top > 14.0, "the tallest solid tops {top} m");
    }
}
