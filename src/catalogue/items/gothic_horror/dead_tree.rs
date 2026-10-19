//! Dead tree - a Gothic-Horror prop: a broken-topped snag, a smooth grey
//! trunk snapped off at 5.15 m into pale splinters, four bare limbs clawing
//! out of it and four bracket fungi shelving up its lower trunk. It is the
//! Understory's `fungus_snag` as it stands in the agent Hypha's region
//! (#1708), and it replaced the catalogue's tapered-cylinder dead tree
//! because the owner judged it the better tree. The slug, role, themes and
//! prosperity band are kept, so every Gothic Horror settlement that names
//! `dead_tree` grows this snag now. The entry draws the generator saved in
//! the Understory's room record, every part reproduced to the
//! ten-thousandth; only what holds each part differs (see Parts).
//!
//! **Parts.** A 5 cm solid cube is the root, standing for the tree's anchor
//! the way Hypha's record has it, and the trunk stands on it; everything
//! else hangs off the trunk at its own place. Hypha's record hangs every
//! part off the anchor, which draws the same tree, but the ruin pass knocks
//! each of a member's top-level parts askew about its own origin, and a limb
//! placed by its stations has its origin at the tree's foot: knocked, it
//! swung up to 0.72 m clear of the trunk. Carried by the trunk, the snag
//! leans, falls or goes as one piece. The trunk is a smooth twelve-sided
//! [`lathe`], solid, flared from
//! 0.62 m at its buried foot to 0.17 m at the break and closed by a point;
//! its bark is a Bark texture turned a quarter (`uv_rotation` 90). The
//! splinters, limbs and twigs are [`spine`]s placed by their stations: five
//! thin pale splinters ring the break, four bark limbs leave the trunk
//! between 2.2 and 4.1 m (two of them three-station and bent, two straight
//! stubs) and two pale twigs fork off the bent limbs' middles. A bracket
//! fungus is a sphere's top half flattened to a shelf over a pale underside
//! flattened further and 4% wider, so the gills show as a rim.
//!
//! Measured with the render tool: 5.95 m tall over the ground (its foot is
//! buried 0.3 m) and 2.86 x 1.83 m across, 3,276 triangles in 21 parts. The
//! spread is lopsided: one bent limb reaches 1.85 m from the trunk's axis,
//! the other limbs stop by 1.5 m. The footprint keeps the old tree's 1.6 m
//! clearance, a spacing circle that this limb overhangs by a quarter of a
//! metre: raising it would move every seeded Gothic Horror settlement that
//! grows a dead tree, for one limb tip.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, lathe, nest, prim, prim_scaled, solid, sphere, spine,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, SovereignBarkConfig, SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

pub struct DeadTree;

impl CatalogueEntry for DeadTree {
    fn slug(&self) -> &'static str {
        "dead_tree"
    }
    fn name(&self) -> &'static str {
        "Dead Tree"
    }
    fn description(&self) -> &'static str {
        "Broken-topped dead snag - bare limbs clawing at the fog, bracket fungi shelving its trunk."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The trunk's silhouette, `(radius, height)` from the buried foot to the
/// point that closes the break.
const TRUNK: [(f32, f32); 10] = [
    (0.0, -0.3),
    (0.62, -0.3),
    (0.5, 0.0),
    (0.38, 0.4),
    (0.33, 1.2),
    (0.29, 2.5),
    (0.24, 3.8),
    (0.2, 4.7),
    (0.17, 5.2),
    (0.0, 5.25),
];

/// The splinters round the break: each a straight spine from its foot, 5 cm
/// thick on the trunk's rim at 5.15 m, out and up to a 1 cm point.
const SPLINTERS: [[f32; 3]; 5] = [
    [0.1533, 5.75, 0.0311],
    [0.0111, 5.5, 0.156],
    [-0.134, 5.95, 0.0806],
    [-0.1135, 5.6, -0.1076],
    [0.0591, 5.82, -0.1448],
];

/// Where each splinter leaves the break, in [`SPLINTERS`]' order.
const SPLINTER_FEET: [[f32; 3]; 5] = [
    [0.1333, 5.15, 0.027],
    [0.0096, 5.15, 0.1357],
    [-0.1165, 5.15, 0.0701],
    [-0.0987, 5.15, -0.0935],
    [0.0514, 5.15, -0.1259],
];

/// A limb or a twig: its stations, `(position, radius)`, from the trunk out.
type Stations = &'static [([f32; 3], f32)];

/// The limbs and twigs in the saved record's order: each bent limb comes
/// after the pale twig that forks off its middle station. `true` marks a
/// bark limb (six sides, three rings a segment), `false` a pale twig (four
/// sides, a straight segment).
const LIMBS: [(bool, Stations); 6] = [
    (
        false,
        &[
            ([0.7963, 3.505, 0.5447], 0.034),
            ([0.9939, 3.955, 0.2559], 0.01),
        ],
    ),
    (
        true,
        &[
            ([0.136, 2.9, 0.093], 0.085),
            ([0.7963, 3.505, 0.5447], 0.0595),
            ([1.4565, 4.0, 1.0965], 0.0255),
        ],
    ),
    (
        false,
        &[
            ([-0.7546, 4.27, 0.2683], 0.03),
            ([-0.6374, 4.72, 0.5981], 0.01),
        ],
    ),
    (
        true,
        &[
            ([-0.1422, 3.5, 0.0506], 0.075),
            ([-0.7546, 4.27, 0.2683], 0.0525),
            ([-1.3671, 4.9, 0.586], 0.0225),
        ],
    ),
    (
        true,
        &[
            ([-0.0153, 4.1, -0.1351], 0.065),
            ([-0.0769, 4.76, -0.6817], 0.0455),
        ],
    ),
    (
        true,
        &[
            ([0.1272, 2.2, -0.1267], 0.07),
            ([0.3753, 2.365, -0.3736], 0.049),
        ],
    ),
];

/// The bracket fungi, lowest first: each cap's radius and the centre it
/// shelves out from.
const BRACKETS: [(f32, [f32; 3]); 4] = [
    (0.26, [0.1639, 0.7, 0.4215]),
    (0.2, [0.0294, 0.95, 0.4146]),
    (0.24, [-0.3602, 1.6, -0.1778]),
    (0.18, [0.101, 2.4, -0.3415]),
];

/// The trunk's and the limbs' bark: a pale grey base colour over a Bark
/// texture's grey-brown, sRGB (0.55, 0.51, 0.46) over (0.16, 0.14, 0.12)
/// (linear RGB here), turned a quarter.
fn bark() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.62, 0.58, 0.52]),
        roughness: Fp(0.95),
        uv_scale: Fp(2.0),
        uv_rotation: Fp(90.0),
        texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
            seed: 877,
            color_light: Fp3([0.2633, 0.2234, 0.1789]),
            color_dark: Fp3([0.022, 0.0174, 0.0134]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Bare, weathered heartwood: the splinters and the twigs.
fn pale_wood() -> SovereignMaterialSettings {
    plain([0.58, 0.54, 0.48], 0.95)
}

fn plain(base_color: [f32; 3], roughness: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(base_color),
        roughness: Fp(roughness),
        ..Default::default()
    }
}

/// One bracket fungus of cap radius `r` at `at`: a russet half-sphere
/// flattened to 0.28 of its height, over a cream underside 4% wider,
/// flattened to 0.12 and set 12 mm lower.
fn bracket(r: f32, at: [f32; 3]) -> [Generator; 2] {
    let cap = with_cut(
        sphere(r, 2, plain([0.62, 0.42, 0.22], 0.85)),
        [0.0, 1.0],
        [0.5, 1.0],
        0.0,
    );
    let underside = sphere(r * 1.04, 2, plain([0.86, 0.74, 0.52], 0.85));
    [
        prim_scaled(cap, at, id_quat(), [1.0, 0.28, 1.0]),
        prim_scaled(
            underside,
            [at[0], at[1] - 0.012, at[2]],
            id_quat(),
            [1.0, 0.12, 1.0],
        ),
    ]
}

fn build_tree() -> Generator {
    let mut parts = Vec::new();
    for (foot, tip) in SPLINTER_FEET.into_iter().zip(SPLINTERS) {
        parts.push(prim(
            spine(&[(foot, 0.05), (tip, 0.01)], 4, 2, pale_wood()),
            [0.0; 3],
            id_quat(),
        ));
    }
    for (is_limb, stations) in LIMBS {
        let kind = if is_limb {
            spine(stations, 6, 3, bark())
        } else {
            spine(stations, 4, 2, pale_wood())
        };
        parts.push(prim(kind, [0.0; 3], id_quat()));
    }
    for (r, at) in BRACKETS {
        parts.extend(bracket(r, at));
    }
    let trunk = prim(solid(lathe(&TRUNK, 12, true, bark())), [0.0; 3], id_quat());
    assemble(vec![
        // The anchor: a 5 cm cube at the foot, the record's root.
        prim(
            solid(cuboid_tapered([0.05; 3], 0.0, Default::default())),
            [0.0; 3],
            id_quat(),
        ),
        nest(trunk, parts),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DeadTree.build(""), "dead_tree");
    }

    /// As built, every limb, splinter and bracket starts inside the trunk
    /// and nothing draws a face where another does.
    #[test]
    fn the_snag_holds_together() {
        let built = DeadTree.build("");
        assert_no_z_fighting(&built, "dead_tree");
        assert_nothing_floats(&built, "dead_tree");
    }

    /// Ruined, the snag keeps its limbs: a fought-over or uneasy settlement
    /// knocks its top-level parts askew one by one, and the trunk carries
    /// every limb, splinter and bracket, so none is knocked off it.
    #[test]
    fn a_ruined_snag_keeps_its_limbs() {
        for escalation in [0.5, 0.95] {
            for seed in 0..64_u64 {
                let mut g = DeadTree.build("");
                crate::pds::ruin::apply_ruin_bounded(
                    &mut g,
                    escalation,
                    seed,
                    DeadTree.ruin_max_lean(),
                );
                assert_nothing_floats(
                    &g,
                    &format!("dead_tree ruined at {escalation}, seed {seed}"),
                );
            }
        }
    }
}
