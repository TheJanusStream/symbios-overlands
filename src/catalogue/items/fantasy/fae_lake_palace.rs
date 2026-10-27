//! Fae lake palace - a Fantasy leader for the shore. A palace of the fair
//! folk at the water's edge: three white stone terraces stepping down
//! toward the lake (-Z), a round domed pavilion on the highest one ringed by
//! slender spires, each spire capped by a softly glowing crystal, and a
//! round moon-gate at the head of the water stair.
//!
//! Each terrace carries what stands on it (its stair, its balustrade, its
//! spires), so a fought-over palace loses a terrace whole rather than
//! leaving its railings in the air.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::gothic_horror::pointed_arch;
use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, plane, prim,
    quat_x, quat_y, railing, solid, sphere, torus, tube, window_card, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_GLASS, CRYSTAL_CYAN, GOLD, MANA_TEAL, crystal, fx, gold, matte, slate, stone};

pub struct FaeLakePalace;

/// Spacing circle: the footing's half diagonal.
const CLEARANCE: f32 = 18.0;
/// The pale stone the fair folk build in.
const WHITE_STONE: [f32; 3] = [0.88, 0.87, 0.84];
/// Moonlit blue-violet slate of the dome and the spire caps.
const MOON_SLATE: [f32; 3] = [0.40, 0.44, 0.62];
/// Balustrade white, a shade warmer than the terraces it stands on.
const RAIL_WHITE: [f32; 3] = [0.90, 0.88, 0.82];

/// One terrace: its half width, front and back (z), and top.
struct Terrace {
    half_x: f32,
    front: f32,
    back: f32,
    top: f32,
}

const T1: Terrace = Terrace {
    half_x: 12.0,
    front: -11.6,
    back: 12.4,
    top: 1.2,
};
const T2: Terrace = Terrace {
    half_x: 10.0,
    front: -6.5,
    back: 12.2,
    top: 3.2,
};
const T3: Terrace = Terrace {
    half_x: 7.2,
    front: -1.5,
    back: 12.0,
    top: 5.2,
};

impl CatalogueEntry for FaeLakePalace {
    fn slug(&self) -> &'static str {
        "fae_lake_palace"
    }
    fn name(&self) -> &'static str {
        "Fae Lake Palace"
    }
    fn description(&self) -> &'static str {
        "White terraces stepping to the water, a domed pavilion, crystal-tipped spires and a moon-gate."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1796).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
    }
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
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

fn white() -> crate::pds::SovereignMaterialSettings {
    stone(WHITE_STONE)
}

/// A flight of `n` steps across `width`, climbing from `y_lo` at `z_front`
/// to the terrace top `y_hi` whose face is at `z_back`. Each step is its own
/// tread's run deep, so no two steps share a face.
fn stairs(width: f32, z_front: f32, z_back: f32, y_lo: f32, y_hi: f32, n: usize) -> Vec<Generator> {
    let run = (z_back - z_front) / n as f32;
    let rise = (y_hi - y_lo) / (n as f32 + 1.0);
    (0..n)
        .map(|k| {
            let top = y_lo + rise * (k as f32 + 1.0);
            let z0 = z_front + run * k as f32;
            prim(
                solid(cuboid_tapered([width, top - y_lo, run], 0.0, white())),
                [0.0, (y_lo + top) * 0.5, z0 + run * 0.5],
                id_quat(),
            )
        })
        .collect()
}

/// A terrace slab and what it carries: a stair up to it from `y_from`, a
/// balustrade along its front either side of a `gap` wide opening, and
/// `extra` parts standing on it.
fn terrace(t: &Terrace, y_from: f32, gap: f32, stair_n: usize, extra: Vec<Generator>) -> Generator {
    let slab = prim(
        solid(cuboid_tapered(
            [t.half_x * 2.0, t.top - y_from, t.back - t.front],
            0.0,
            white(),
        )),
        [0.0, (y_from + t.top) * 0.5, (t.front + t.back) * 0.5],
        id_quat(),
    );
    let mut parts = Vec::new();
    // A gold-capped coping band round the top, proud of the slab faces.
    parts.push(prim(
        solid(cuboid_tapered(
            [t.half_x * 2.0 + 0.16, 0.18, t.back - t.front + 0.16],
            0.0,
            white(),
        )),
        [0.0, t.top - 0.11, (t.front + t.back) * 0.5],
        id_quat(),
    ));
    let run = 0.4 * stair_n as f32;
    parts.extend(stairs(
        gap - 0.3,
        t.front - run,
        t.front - 0.08,
        y_from,
        t.top,
        stair_n,
    ));
    let rail_z = t.front + 0.35;
    let rail = matte(RAIL_WHITE);
    for s in [-1.0_f32, 1.0] {
        parts.extend(railing(
            [s * (gap * 0.5 + 0.1), t.top, rail_z],
            [s * (t.half_x - 0.35), t.top, rail_z],
            1.0,
            0.6,
            rail.clone(),
        ));
    }
    parts.extend(extra);
    nest(slab, parts)
}

/// A slender spire on a terrace at `foot`: a drum, a tapering shaft, a gold
/// band, a slate cap and a softly glowing crystal finial.
fn spire(foot: [f32; 3], r: f32, shaft_h: f32, cap_h: f32) -> Generator {
    let [x, y, z] = foot;
    let drum_h = 0.5;
    let top = y + drum_h + shaft_h;
    let shaft = prim(
        solid(cylinder_tapered(r, shaft_h, 12, 0.12, white())),
        [x, y + drum_h + shaft_h * 0.5, z],
        id_quat(),
    );
    let r_top = r * 0.88;
    let cap = nest(
        prim(
            solid(cone(r_top + 0.18, cap_h, 12, slate(MOON_SLATE))),
            [x, top + cap_h * 0.5, z],
            id_quat(),
        ),
        vec![crystal(
            [x, top + cap_h - 0.2, z],
            r * 0.42,
            r * 2.1,
            id_quat(),
            glow(CRYSTAL_CYAN, 1.6),
        )],
    );
    let band = prim(
        torus(0.08, r_top + 0.02, gold(GOLD)),
        [x, top - 0.05, z],
        id_quat(),
    );
    nest(
        prim(
            solid(cylinder_tapered(r + 0.2, drum_h, 12, 0.0, white())),
            [x, y + drum_h * 0.5, z],
            id_quat(),
        ),
        vec![nest(shaft, vec![band, cap])],
    )
}

/// An arched opening on the pavilion's round wall at yaw `a` (0 = straight
/// to -Z): a lit chamber behind a window card, a sill, and a pointed gold
/// arch head - built facing -Z and turned on its sill.
fn window(r: f32, a: f32, sill_y: f32, half_w: f32, h: f32) -> Generator {
    let zf = -r;
    let sill = prim(
        solid(cuboid_tapered(
            [half_w * 2.0 + 0.16, 0.12, 0.22],
            0.0,
            white(),
        )),
        [0.0, sill_y - 0.06, zf - 0.06],
        id_quat(),
    );
    let mut parts = vec![
        prim(
            solid(cuboid_tapered(
                [half_w * 2.0, h + half_w * 0.9, 0.1],
                0.0,
                lit_from_dusk(glow(ARCANE_GLASS, 2.0)),
            )),
            [0.0, sill_y + (h + half_w * 0.9) * 0.5, zf - 0.02],
            id_quat(),
        ),
        prim(
            plane(
                [half_w * 2.0 - 0.08, h - 0.05],
                window_card(GOLD, 2, 4, 0.35, 0.06),
            ),
            [0.0, sill_y + h * 0.5, zf - 0.09],
            quat_x(-FRAC_PI_2),
        ),
    ];
    parts.extend(pointed_arch(
        [0.0, sill_y + h, zf - 0.07],
        half_w + 0.04,
        0.07,
        gold(GOLD),
    ));
    let mut w = nest(sill, parts);
    // Turn the whole opening round the wall to its yaw.
    let [x, y, z] = w.transform.translation.0;
    let (s, c) = a.sin_cos();
    w.transform.translation.0 = [x * c + z * s, y, -x * s + z * c];
    w.transform.rotation = quat_y(a);
    w
}

/// The round pavilion on the top terrace: a drum with lit arched windows
/// and an arched door to -Z, a gold cornice, a slate dome and a central
/// spire with a crystal finial.
fn pavilion(base_y: f32, cz: f32) -> Generator {
    let r = 4.2;
    let drum_h = 6.0;
    let top = base_y + drum_h;
    let mut parts = vec![prim(
        solid(torus(0.18, r + 0.06, gold(GOLD))),
        [0.0, top, cz],
        id_quat(),
    )];
    // A plinth course round its foot.
    parts.push(prim(
        solid(cylinder_tapered(r + 0.25, 0.45, 32, 0.0, white())),
        [0.0, base_y + 0.225, cz],
        id_quat(),
    ));
    // Door at the front, lit from within, under a gold pointed arch.
    let zf = cz - r;
    parts.push(prim(
        solid(cuboid_tapered(
            [1.8, 3.4, 0.12],
            0.0,
            lit_from_dusk(glow(MANA_TEAL, 1.1)),
        )),
        [0.0, base_y + 0.45 + 1.7, zf - 0.04],
        id_quat(),
    ));
    // Two pale doors standing ajar across the light.
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.62, 2.5, 0.08], 0.0, matte(RAIL_WHITE))),
            [s * 0.5, base_y + 0.45 + 1.25, zf - 0.15],
            id_quat(),
        ));
    }
    parts.extend(pointed_arch(
        [0.0, base_y + 0.45 + 2.6, zf - 0.11],
        0.95,
        0.1,
        gold(GOLD),
    ));
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.2, 2.6, 0.2], 0.0, gold(GOLD))),
            [s * 0.95, base_y + 0.45 + 1.3, zf - 0.12],
            id_quat(),
        ));
    }
    // Lit windows round the front half, a lower and an upper rank.
    for a in [-1.0_f32, -0.55, 0.55, 1.0] {
        let mut w = window(r, a, base_y + 1.4, 0.42, 1.7);
        w.transform.translation.0[2] += cz;
        parts.push(w);
    }
    for a in [-0.75_f32, 0.0, 0.75] {
        let mut w = window(r, a, base_y + 4.3, 0.34, 1.1);
        w.transform.translation.0[2] += cz;
        parts.push(w);
    }
    // Dome and central spire.
    let dome_r = r + 0.15;
    let spire = nest(
        prim(
            solid(cone(0.45, 4.0, 12, slate(MOON_SLATE))),
            [0.0, top + dome_r - 0.15 + 2.0, cz],
            id_quat(),
        ),
        vec![crystal(
            [0.0, top + dome_r + 3.6, cz],
            0.3,
            1.6,
            id_quat(),
            glow(CRYSTAL_CYAN, 1.7),
        )],
    );
    parts.push(nest(
        prim(
            solid(with_cut(
                sphere(dome_r, 6, slate(MOON_SLATE)),
                [0.0, 1.0],
                [0.5, 1.0],
                0.0,
            )),
            [0.0, top, cz],
            id_quat(),
        ),
        vec![spire],
    ));
    nest(
        prim(
            solid(cylinder_tapered(r, drum_h, 32, 0.0, white())),
            [0.0, base_y + drum_h * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// The moon-gate: a round stone ring standing on the lowest terrace at the
/// head of the water stair, a gold ring inside it and a crystal at its
/// crown, with fae lamps on gold posts either side.
fn moon_gate(floor: f32, z: f32) -> Vec<Generator> {
    let (ro, ri) = (2.4, 1.9);
    let cy = floor + 2.0;
    let ring = nest(
        prim(
            solid(tube(ro, ri, 0.7, 40, white())),
            [0.0, cy, z],
            quat_x(-FRAC_PI_2),
        ),
        vec![],
    );
    let inner = prim(
        torus(0.06, ri + 0.02, gold(GOLD)),
        [0.0, cy, z - 0.3],
        quat_x(-FRAC_PI_2),
    );
    let crown = crystal(
        [0.0, cy + ro - 0.12, z],
        0.22,
        1.0,
        id_quat(),
        glow(CRYSTAL_CYAN, 1.6),
    );
    let mut out = vec![ring, inner, crown];
    for s in [-1.0_f32, 1.0] {
        let x = s * (ro + 0.55);
        out.push(nest(
            prim(
                solid(cylinder_tapered(0.07, 1.8, 8, 0.0, gold(GOLD))),
                [x, floor + 0.9, z],
                id_quat(),
            ),
            vec![prim(
                sphere(0.16, 4, glow(MANA_TEAL, 2.2)),
                [x, floor + 1.92, z],
                id_quat(),
            )],
        ));
    }
    out
}

fn build_tree() -> Generator {
    let pav_z = 4.6;
    let t3 = terrace(
        &T3,
        T2.top,
        3.0,
        6,
        vec![
            pavilion(T3.top, pav_z),
            spire([-6.3, T3.top, 0.0], 0.6, 8.6, 3.6),
            spire([6.3, T3.top, 0.0], 0.6, 8.6, 3.6),
            spire([-6.3, T3.top, 10.8], 0.6, 8.6, 3.6),
            spire([6.3, T3.top, 10.8], 0.6, 8.6, 3.6),
        ],
    );
    let t2 = terrace(
        &T2,
        T1.top,
        4.0,
        6,
        vec![
            spire([-8.9, T2.top, -5.4], 0.42, 5.4, 2.6),
            spire([8.9, T2.top, -5.4], 0.42, 5.4, 2.6),
        ],
    );
    let gate_z = T1.front + 0.45;
    let t1 = terrace(&T1, 0.0, 6.2, 4, moon_gate(T1.top, gate_z));

    let mut root = footing(24.0, 26.0, [0.0, -0.4], CLEARANCE);
    attach(&mut root, t1);
    attach(&mut root, t2);
    attach(&mut root, t3);
    attach(
        &mut root,
        fx::mana_motes([0.0, T3.top + 1.0, pav_z - 6.0], 0xFAE1_A4E0),
    );
    root.audio = fx::fae_chime();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FaeLakePalace.build(""), "fae_lake_palace");
    }

    #[test]
    fn nothing_z_fights() {
        assert_no_z_fighting(&FaeLakePalace.build(""), "fae_lake_palace");
    }

    #[test]
    fn nothing_floats() {
        assert_nothing_floats(&FaeLakePalace.build(""), "fae_lake_palace");
    }

    #[test]
    fn its_crystals_glow() {
        assert!(has_emissive(&FaeLakePalace.build("")));
    }

    #[test]
    fn record_stays_inside_the_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&FaeLakePalace.build(""))
            .expect("serialises");
        eprintln!("fae_lake_palace record: {bytes} B");
        assert!(bytes < 80 * 1024, "{bytes} B");
    }
}
